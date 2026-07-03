# Using Local whatsapp-rust (Patched Dependency)

Use a local checkout of `whatsapp-rust` instead of the git dependency so you can modify and test changes immediately.

## Why

- Fix the inner loop's blind sleep (fibonacci backoff uninterruptible)
- Test changes to pairing, reconnection, or protocol logic without pushing to git
- Iterate faster — edit `whatsapp-rust/src/...`, run `cargo check`, repeat

## How

Add a `[patch]` section to `waos/Cargo.toml` that overrides the git deps with local paths:

```toml
[workspace.dependencies]
# keep the git deps as-is (lines 16-22) for documentation / fallback:
whatsapp-rust = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
wacore = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
wacore-binary = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
waproto = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
whatsapp-rust-tokio-transport = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
whatsapp-rust-ureq-http-client = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }

# ... other deps ...

# At the bottom of the file, add:
[patch."https://github.com/oxidezap/whatsapp-rust"]
whatsapp-rust = { path = "../whatsapp-rust" }
wacore = { path = "../whatsapp-rust/wacore" }
wacore-binary = { path = "../whatsapp-rust/wacore/binary" }
waproto = { path = "../whatsapp-rust/waproto" }
whatsapp-rust-tokio-transport = { path = "../whatsapp-rust/transports/tokio-transport" }
whatsapp-rust-ureq-http-client = { path = "../whatsapp-rust/http_clients/ureq-client" }
```

The `[patch]` section tells Cargo: *"use the local path instead of the git URL for these crates"*. The git deps remain declared as workspace dependencies for documentation; Cargo resolves to the local path at build time.

## Updating to a new upstream version

1. Update the local checkout:
   ```bash
   cd ../whatsapp-rust
   git pull  # or: git checkout <new-tag-or-rev>
   ```
2. Update the `rev` in `[workspace.dependencies]` to match (for documentation).
3. Run `cargo check` — Cargo picks up the new code automatically.

If you clone a completely new version, just point the `[patch]` paths to the new directory.

## Patch 1: Interruptible Reconnect Sleep

**File:** `whatsapp-rust/src/client/lifecycle.rs` (~line 379)

Replace bare `self.runtime.sleep(delay).await` with 1-second chunks that check `self.is_running`:

```rust
const GRANULARITY: std::time::Duration = std::time::Duration::from_secs(1);
let mut remaining = delay;
while remaining > std::time::Duration::ZERO
    && self.is_running.load(Ordering::Relaxed)
{
    let chunk = GRANULARITY.min(remaining);
    self.runtime.sleep(chunk).await;
    remaining = remaining.saturating_sub(chunk);
}
```

This makes `disconnect()` take effect within 1s instead of waiting for the full fibonacci backoff (up to 15 min).

## Patch 2: Emit StreamError for 503

**File:** `whatsapp-rust/src/client/node_io.rs` (~line 1085)

Dispatch an `Event::StreamError` so the WAOS outer loop can apply a cooldown after 503:

```rust
"503" => {
    info!("Got 503 service unavailable, will auto-reconnect.");
    self.is_logged_in.store(false, Ordering::Relaxed);
    self.core.event_bus.dispatch(Event::StreamError(
        crate::types::events::StreamError {
            code: code.to_string(),
            raw: Some(node.to_owned()),
        },
    ));
}
```

## Patch 3: Reachout Timelock Feature

**Files:**
- `whatsapp-rust/src/features/reachout_timelock.rs` — new
- `whatsapp-rust/src/features/mod.rs` — added `mod reachout_timelock` + `pub use`

Adds `client.reachout_timelock().fetch()` to query account reach-out timelock status via MEX (DOC_ID=23983697327930364). WAOS uses this after `Event::Connected` to proactively detect 463 restrictions.

## Patch 4: require_full_sync = false

**File:** `wacore/src/store/device.rs:208`

```rust
// Before:  require_full_sync: Some(true),
// After:   require_full_sync: Some(false),
```

WA Web sets `requireFullSync` dynamically via gkx flags. Hardcoding `true` was wrong per reverse-engineered findings ("should NOT be hardcoded `true`"). `false` avoids unnecessary full-sync requests.

## Patch 5: Status ACK Wait (don't mark posted on nack)

**File:** `whatsapp-rust/src/send/mod.rs` (~line 879)

Replace fire-and-forget `spawn_phash_validation` with inline ack wait so `send_status_message` only returns `Ok` when the server accepts the status:

```rust
if let Some((rx, phash)) = ack {
    match wacore::runtime::timeout(
        &*self.runtime,
        std::time::Duration::from_secs(10),
        rx,
    )
    .await
    {
        Ok(Ok(ack_node)) => {
            if let Some(error_code) = ack_node.get_attr("error") {
                return Err(SendError::InvalidRequest(
                    format!("server rejected status (error {})", error_code.as_str())
                ));
            }
            if let Some(server_phash) = ack_node.get_attr("phash") {
                let server = server_phash.as_str().into_owned();
                if server != phash {
                    self.handle_phash_mismatch(&to, &phash, &server, true).await;
                }
            }
        }
        Ok(Err(_)) => {
            self.response_waiters.lock().await.remove(&request_id);
            log::warn!("ack channel closed for status {}", request_id);
        }
        Err(_) => {
            self.response_waiters.lock().await.remove(&request_id);
            log::warn!("ack timed out for status {} (may still be delivered)", request_id);
        }
    }
}
```

Without this, the server 479 nack arrives after `send_status_message` already returned `Ok`, so waos stores the status as "posted" in `status_history` even though WhatsApp rejected it.

## Files that need changes

| File | Change |
|------|--------|
| `waos/Cargo.toml` | Add `[patch]` section (this doc) |
| `whatsapp-rust/src/client/lifecycle.rs:379` | Patch 1 — interruptible sleep |
| `whatsapp-rust/src/client/node_io.rs:1085` | Patch 2 — 503 StreamError |
| `whatsapp-rust/src/features/reachout_timelock.rs` | Patch 3 — new: MEX timelock query |
| `whatsapp-rust/src/features/mod.rs` | Patch 3 — register module |
| `wacore/src/store/device.rs:208` | Patch 4 — require_full_sync = false |
| `whatsapp-rust/src/send/mod.rs:879` | Patch 5 — status ack wait |
