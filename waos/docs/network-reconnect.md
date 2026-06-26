# Network Reconnect — Blind Sleep Problem

## Problem

When DNS goes down, the client enters an uninterruptible fibonacci backoff sleep and never wakes up early when the network comes back.

Logs showing the failure cascade:

```
ERROR whatsapp_rust::client: Failed to connect: WebSocket connect failed: client DNS lookup failed. Will retry...
INFO  whatsapp_rust::client: Will attempt to reconnect in 20.734s (attempt 8)
INFO  whatsapp_rust::pair: All QR codes for this session have expired.
INFO  whatsapp_rust::client: Disconnecting client intentionally.
INFO  whatsapp_rust::client: Client run loop has shut down.
```

## Where the errors come from

All messages originate from the **whatsapp-rust** crate (git dependency):

| Log message | File (whatsapp-rust) | Line |
|---|---|---|
| `Failed to connect: WebSocket connect failed... Will retry...` | `src/client/lifecycle.rs` | 316 |
| `Will attempt to reconnect in {delay} (attempt {n})` | `src/client/lifecycle.rs` | 374-377 |
| `All QR codes for this session have expired.` | `src/pair.rs` | 130 |
| `Disconnecting client intentionally.` | `src/client/lifecycle.rs` | 528 |
| `Client run loop has shut down.` | `src/client/lifecycle.rs` | 381 |

## Architecture

There are two reconnection layers:

```
waos run_bot()                         ← outer loop (retries with 3^n backoff, max 300s)
  └─ bot.run() → Client::run()         ← inner loop (fibonacci backoff, max 900s)
       └─ read_messages_loop()
            └─ connect() → reconnect on failure
```

### Inner loop (whatsapp-rust, `lifecycle.rs:369-379`)

```
error_count = auto_reconnect_errors.fetch_add(1)
delay = fibonacci_backoff(error_count)     // 1s, 1s, 2s, 3s, ... up to 900s
self.runtime.sleep(delay).await            // 🔴 BLIND — no is_running check, no DNS check
```

This sleep is a bare `tokio::time::sleep()` with **no cancellation mechanism**:
- `disconnect()` sets `is_running = false` but does NOT interrupt the sleep
- No DNS/network health check during the sleep
- The while-loop condition only re-checks `is_running` AFTER the sleep completes

### Outer loop (waos, `account_manager.rs`)

```
// Before fix:
tokio::time::sleep(Duration::from_secs(delay)).await  // 🔴 Also blind
```

## WAOS-Level Fix (applied)

### Files changed

`waos/lib/src/adapter/account_manager.rs`

### What was added

1. **`is_network_available()`** — probes DNS via `tokio::net::lookup_host("wss.web.whatsapp.com:443")`
2. **`wait_for_network(max_delay, alias)`** — sleeps in 5s chunks, checks DNS after each chunk, returns early when DNS resolves
3. **DNS gate at outer loop entry** — before creating store/bot, if DNS is down, waits up to 30s for recovery (avoids wasting retries on dead network)
4. **DNS monitor in `tokio::select!`** — runs alongside `bot_handle`. After 3 consecutive failed DNS checks (~30s), it calls `bot_handle.abort()` which **immediately kills the background client task** (cancelling any in-progress fibonacci sleep). The outer loop then enters `wait_for_network(300s)` which probes DNS every 5s and retries as soon as the network recovers.
5. **`&mut bot_handle` as a select branch** — so when the client run loop exits naturally (QR expiry, disconnect, etc.), the outer loop wakes up immediately instead of being stuck on `logout_rx` forever.

### Fixed flow

```
DNS goes down
  → connect() fails → inner loop starts fibonacci backoff (e.g. 21s)
  → DNS monitor detects 3 consecutive failures (~30s total)
  → bot_handle.abort()  ← kills client task, sleep cancelled immediately
  → outer loop catches abort → wait_for_network(300s)
  → probes DNS every 5s
  → DNS comes back → immediately creates fresh Bot → client reconnects
```

All bare `tokio::time::sleep(...)` calls in `run_bot()` were replaced with `wait_for_network(...)`, so every retry backoff across the entire function self-interrupts when network recovers.

## Upstream Fix Needed

The WAOS-level fix handles the case where the outer loop has taken control. But the **inner loop's blind sleep** (`lifecycle.rs:379`) is still unfixed — it sits there until either the full sleep elapses or `bot_handle.abort()` is called from outside.

For a complete fix, the upstream `whatsapp-rust` crate needs two changes:

### 1. Make the reconnect sleep interruptible

Replace the bare sleep in `src/client/lifecycle.rs` (~line 379):

```rust
// BEFORE:
self.runtime.sleep(delay).await;

// AFTER:
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

This way:
- `disconnect()` (which sets `is_running = false`) takes effect within 1 second
- The outer loop's `bot_handle.abort()` still works as immediate kill
- No additional dependencies needed

### 2. (Optional) Add DNS check in the sleep loop

For self-sufficient operation without an external monitor, add a lightweight DNS check using the existing `Runtime::spawn_blocking`:

```rust
async fn check_dns(&self) -> bool {
    self.runtime
        .spawn_blocking(Box::new(|| {
            std::net::ToSocketAddrs::to_socket_addrs("wss.web.whatsapp.com:443").is_ok()
        }))
        .await
        .unwrap_or(false)
}
```

Then in the sleep loop, also check DNS and reset the backoff counter when it recovers:

```rust
if remaining > Duration::ZERO && self.check_dns().await {
    info!("Network connectivity restored, resetting backoff.");
    self.auto_reconnect_errors.store(0, Ordering::Relaxed);
    break;
}
```

### Files to modify

| File | Change |
|------|--------|
| `whatsapp-rust/src/client/lifecycle.rs:369-379` | Replace bare sleep with interruptible chunked sleep + optional DNS check |
| `whatsapp-rust/src/client.rs` (optional) | Add `check_dns()` method |

### PR to upstream

The fix should be submitted as a PR to https://github.com/oxidezap/whatsapp-rust (or the upstream fork used by WAOS). The change is minimal, backward-compatible, and fixes a real production issue where clients get stuck for up to 15 minutes during network interruptions.

See reference implementation in `whatsapp-rust/src/client/lifecycle.rs` in this repo for the exact code to change.

## Summary

| Layer | Status | What |
|-------|--------|------|
| WAOS outer loop (`account_manager.rs`) | ✅ Fixed | DNS monitor, `wait_for_network`, abort stuck bot |
| whatsapp-rust inner loop (`lifecycle.rs`) | ❌ Needs upstream PR | Make sleep check `is_running` in short intervals |
