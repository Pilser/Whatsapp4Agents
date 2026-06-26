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

## What to fix in whatsapp-rust

The key fix for the reconnect blind-sleep problem is in `whatsapp-rust/src/client/lifecycle.rs` (~line 379):

```rust
// Replace this:
self.runtime.sleep(delay).await;

// With interruptible sleep:
const SLEEP_GRANULARITY: std::time::Duration = std::time::Duration::from_secs(1);
let mut remaining = delay;
while remaining > std::time::Duration::ZERO
    && self.is_running.load(Ordering::Relaxed)
{
    let chunk = SLEEP_GRANULARITY.min(remaining);
    self.runtime.sleep(chunk).await;
    remaining -= chunk;
}
```

This makes `disconnect()` take effect within 1 second instead of waiting for the full fibonacci backoff (up to 15 minutes).

## Files that need changes

| File | Change |
|------|--------|
| `waos/Cargo.toml` | Add `[patch]` section (this doc) |
| `whatsapp-rust/src/client/lifecycle.rs:379` | Replace bare sleep with interruptible loop |
