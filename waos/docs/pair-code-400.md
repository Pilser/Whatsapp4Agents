# Pair Code 400 Bad-Request

## Problem

`POST /api/accounts/{alias}/pair` returns `PairError::RequestFailed(ServerError { code: 400, text: "bad-request" })` when calling `Client::pair_with_code()`.

The noise handshake (XX pattern) succeeds and the WebSocket connects reliably — QR regeneration works fine. Only the companion_hello IQ sent by `pair_with_code()` is rejected.

## Root Cause

The companion_hello IQ is rejected because the **registration payload sent during the XX noise handshake's msg3 (`ClientPayload`)** signals "QR pairing mode" to the server before the companion_hello IQ is ever sent.

### Detailed flow

1. `connect_graph()` calls `do_handshake()` which calls `run_xx_handshake()`
2. `run_xx_handshake()` builds `client_payload` via `device.get_client_payload()` (device.rs:430)
3. For unpaired devices (`pn` is `None`), this returns `get_registration_payload()` (device.rs:458-505), which includes a full `DevicePairingRegistrationData` protobuf with `e_regid`, `e_keytype`, `e_ident`, `e_skey_val`, `e_skey_sig`, `build_hash`, `device_props`
4. This payload tells the server: **"this is a new device registering via QR pairing"**
5. The server flags the connection as QR-pairing mode
6. When `pair_with_code()` later sends the companion_hello IQ, the server rejects it because the connection is already in QR mode → 400 bad-request

### Companion_hello IQ format

```
<iq xmlns="md" type="set" to="s.whatsapp.net" id="...">
  <link_code_companion_reg jid="PHONE@s.whatsapp.net" stage="companion_hello"
                           should_show_push_notification="true">
    <link_code_pairing_wrapped_companion_ephemeral_pub>80 bytes (salt||iv||encrypted_ephemeral)</link_code_pairing_wrapped_companion_ephemeral_pub>
    <companion_server_auth_key_pub>32 bytes (noise_static_pub)</companion_server_auth_key_pub>
    <companion_platform_id>e.g. "1"</companion_platform_id>
    <companion_platform_display>e.g. "Chrome (Linux)"</companion_platform_display>
    <link_code_pairing_nonce>0</link_code_pairing_nonce>
  </link_code_companion_reg>
</iq>
```

### Why the wa-rs internal `with_pair_code` also failed

The removed internal flow (bot.rs:433-462) had the exact same issue — it called `persistence_manager.get_device_snapshot()` → `get_client_payload()` → `get_registration_payload()` during the handshake, putting the connection in QR mode. The 400 error occurred there too, but was silently swallowed by the `warn!()` log at bot.rs:458 (no event dispatch).

## Fix (requires wa-rs change)

Modify `get_client_payload()` in `wacore/src/store/device.rs` to accept a flag that causes it to return a **pair-code-specific payload** without `device_pairing_data`. For example:

1. Add `registering_with_pair_code: bool` field to `Device` struct
2. In `get_client_payload()`:
   - `pn.is_some()` → `get_login_payload()` (existing, no change)
   - `pn.is_none() && !self.registering_with_pair_code` → `get_registration_payload()` (existing, QR mode)
   - `pn.is_none() && self.registering_with_pair_code` → return a bare `ClientPayload` **without** `device_pairing_data` (pair code mode)
3. Before calling `bot.run()`, set this flag via `PersistenceManager::process_command(DeviceCommand::...)`

Without this change, the server always interprets the initial connection as a QR-pairing attempt and rejects subsequent companion_hello IQs.

## Files

| File | Purpose |
|------|---------|
| `whatsapp-rust/wacore/src/store/device.rs:430-505` | `get_client_payload()` dispatches to `get_registration_payload()` for unpaired devices — this sends QR-mode protobuf |
| `whatsapp-rust/wacore/src/store/device.rs:212-286` | `Device` struct — add `registering_with_pair_code` field here |
| `whatsapp-rust/wacore/src/store/device.rs:332-383` | `Device::new()` — initialize the new field |
| `whatsapp-rust/src/handshake.rs:243-276` | `run_xx_handshake()` reads `device.get_client_payload()` during noise handshake msg3 |
| `whatsapp-rust/src/pair_code.rs:116-238` | `Client::pair_with_code()` — builds and sends companion_hello IQ |
| `whatsapp-rust/wacore/src/pair_code.rs:308-355` | `build_companion_hello_iq()` — constructs the IQ nodes |
| `waos/lib/src/adapter/account_manager.rs:581` | WAOS stores client in `pending_clients` after `builder.build()` — set the pair_code flag here before calling `bot.run()` |
| `waos/bin/src/api/accounts.rs:117-210` | `pair()` handler — polls `pending_clients`, calls `pair_with_code()` |

## Next Steps

1. Add `registering_with_pair_code` flag to `Device` struct + `DeviceCommand`
2. Modify `get_client_payload()` to omit `device_pairing_data` when flag is set
3. Set flag in `run_bot()` before `bot.run()` when pairing (not QR)
4. Rebuild and test
