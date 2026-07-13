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
whatsapp-rust = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }
wacore = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }
wacore-binary = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }
waproto = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }
whatsapp-rust-tokio-transport = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }
whatsapp-rust-ureq-http-client = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "4def65976a8d7ab7f429d6e3300201026f795b15" }

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

## Patch 6: LID-to-PN Conversion for Status

Two parts:

### 6a: `wacore/src/send/status.rs` — `<to>` nodes in `ensure_status_participants`

**Before** (~line 21):
```rust
.map(|jid| NodeBuilder::new("to").attr("jid", jid.to_non_ad()).build())
```

**After** (converts LID JIDs to PN):
```rust
.map(|jid| {
    let user_jid = if jid.is_lid() {
        group_info
            .phone_jid_for_lid_user(&jid.user)
            .cloned()
            .unwrap_or_else(|| jid.to_non_ad())
    } else {
        jid.to_non_ad()
    };
    NodeBuilder::new("to").attr("jid", user_jid).build()
})
```

### 6b: `src/send/mod.rs` — LID→PN map for LID recipients

Adds a second loop in `send_status_message` after the existing PN→LID
resolution to also map LID recipients back to their PN:
```rust
for (i, jid) in recipients.iter().enumerate() {
    if jid.is_lid() {
        if let Some(lid_jid) = &resolved[i] {
            if let Ok(Some(entry)) = self.get_lid_pn_entry(jid).await {
                lid_to_pn_map.insert(
                    lid_jid.user.clone(),
                    Jid::pn(entry.phone_number.as_ref()),
                );
            }
        }
    }
}
```

## Patch 7: Always Include `<device-identity>` for Status

**File:** `src/send/mod.rs` (inside `send_status_message`, ~line 836)

WhatsApp Web always includes `<device-identity>` in the status stanza.
Our code only added it when a prekey message was emitted (new device
needing sender key). This builds the node unconditionally from the ADV
account:

```rust
let mut extra_stanza_nodes = Vec::new();

// Always include <device-identity> for status (WA Web always has it).
if let Some(account) = account_info {
    extra_stanza_nodes.push(
        NodeBuilder::new("device-identity")
            .bytes(account.encode_to_vec())
            .build(),
    );
}

if wacore::send::status_carries_privacy_meta(&message) {
    extra_stanza_nodes.push(
        NodeBuilder::new("meta")
            .attr("status_setting", options.privacy.as_str())
            .attr("session_scope", "status")
            .build(),
    );
}
```

## Patch 8: Add `session_scope="status"` to `<meta>`

**File:** `src/send/mod.rs` (same location as Patch 7)

Added `.attr("session_scope", "status")` to the `<meta>` node. Applied
together with Patch 7 in the same edit.

## Patch 9: ImageOptions — Add height/width/thumbnail fields

**File:** `whatsapp-rust/src/media.rs`

Added fields to `ImageOptions`:

```rust
pub struct ImageOptions {
    pub caption: Option<String>,
    pub mimetype: Option<String>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
    pub context_info: Option<Box<wa::ContextInfo>>,
    // --- new ---
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub view_once: Option<bool>,
    pub thumbnail_direct_path: Option<String>,
    pub thumbnail_sha256: Option<Vec<u8>>,
    pub thumbnail_enc_sha256: Option<Vec<u8>>,
}
```

Also updated `image_message()` to pass new fields into the protobuf (`height`, `width`, `view_once`, `thumbnail_direct_path`, `thumbnail_sha256`, `thumbnail_enc_sha256`).

## Patch 10: VideoOptions — Add height/width/thumbnail fields

**File:** `whatsapp-rust/src/media.rs`

Added fields to `VideoOptions`:

```rust
pub struct VideoOptions {
    pub caption: Option<String>,
    pub mimetype: Option<String>,
    pub jpeg_thumbnail: Option<Vec<u8>>,
    pub duration_seconds: Option<u32>,
    pub gif_playback: Option<bool>,
    pub context_info: Option<Box<wa::ContextInfo>>,
    // --- new ---
    pub height: Option<u32>,
    pub width: Option<u32>,
    pub view_once: Option<bool>,
    pub gif_attribution: Option<i32>,
    pub thumbnail_direct_path: Option<String>,
    pub thumbnail_sha256: Option<Vec<u8>>,
    pub thumbnail_enc_sha256: Option<Vec<u8>>,
}
```

Also updated `video_message()` to pass new fields into the protobuf (`height`, `width`, `view_once`, `gif_attribution` via `from_i32`, `thumbnail_direct_path`, `thumbnail_sha256`, `thumbnail_enc_sha256`).

## Patch 11: status.rs — send_image/send_video accept media_opts

**File:** `whatsapp-rust/src/features/status.rs`

Both `send_image` and `send_video` now take an extra `media_opts` parameter
(`ImageOptions` / `VideoOptions`) that merges with the inline caption and
thumbnail so callers can pass dimensions, CDN thumbnail fields, etc.

```rust
pub async fn send_image(
    &self, upload: UploadResponse, thumbnail: Vec<u8>,
    caption: Option<&str>, recipients: &[Jid],
    options: StatusSendOptions,
    media_opts: crate::media::ImageOptions,   // <-- new
) -> Result<SendResult, SendError>;

pub async fn send_video(
    &self, upload: UploadResponse, thumbnail: Vec<u8>,
    duration_seconds: u32, caption: Option<&str>,
    recipients: &[Jid], options: StatusSendOptions,
    media_opts: crate::media::VideoOptions,   // <-- new
) -> Result<SendResult, SendError>;
```

## Proto descriptor regeneration

**File:** `waproto/src/whatsapp.desc` + `waproto/src/whatsapp.desc.sha256`

The proto build pipeline (see `waproto/build.rs`) reads the `.desc` file at build time to generate Rust structs. After editing `whatsapp.proto`, run:

```bash
bash scripts/regenerate-proto-desc.sh
```

This wraps `protoc --descriptor_set_out` to produce a binary descriptor from the `.proto`, then writes the new SHA-256 checksum. `build.rs` verifies the checksum on every build — if proto and desc are out of sync, compilation fails with a hash mismatch error.

Regenerated for Patch 12 because `DeviceListMetadata` fields 1-3 (`senderKeyHash`, `senderTimestamp`, `senderKeyIndexes`) were added to the proto and the desc had not been refreshed.

## Patch 12: DeviceListMetadata in MessageContextInfo

**File:** `wacore/src/reporting_token.rs:662`

`reporting_context_info()` now accepts `account: Option<&wa::ADVSignedDeviceIdentity>` and populates:
- `deviceListMetadata.senderKeyHash` = SHA-256 of the identity public key
- `deviceListMetadata.senderTimestamp` = timestamp from decoded `ADVDeviceIdentity.details`
- `deviceListMetadata.senderKeyIndexes` = key index from decoded `ADVDeviceIdentity`
- `deviceListMetadataVersion = 2` on `MessageContextInfo`

Updated callers in `wacore/src/send/dm.rs:99` and `wacore/src/send/group.rs:175` to pass `account`.

## Patch 13 — REVERTED

`<device-identity>` placement was kept at message level (original behavior). WA Web JS incoming parser reads `maybeChild("device-identity")` at the **message stanza level**, not inside `<to>`. Server-side normalization likely handles both, but original proven placement is safer.

## Patch 14 — REVERTED

`addressing_mode` on group stanzas was restored to original behavior. WA Web JS bundle
could not confirm whether the attribute is sent or not (`[UNVERIFIED]` per audit). Original
code was working, so reverted to avoid introducing risk.

## Files that need changes

| File | Change | Patch |
|------|--------|-------|
| `waos/Cargo.toml` | Add `[patch]` section | — |
| `whatsapp-rust/src/client/lifecycle.rs:379` | Interruptible sleep | 1 |
| `whatsapp-rust/src/client/node_io.rs:1085` | 503 StreamError | 2 |
| `whatsapp-rust/src/features/reachout_timelock.rs` | New: MEX timelock query | 3 |
| `whatsapp-rust/src/features/mod.rs` | Register module | 3 |
| `wacore/src/store/device.rs:208` | require_full_sync = false | 4 |
| `whatsapp-rust/src/send/mod.rs:879` | Status ack wait | 5 |
| `wacore/src/send/status.rs:21` | LID-to-PN in `<to>` nodes | 6a |
| `whatsapp-rust/src/send/mod.rs:761` | LID→PN map for LID recipients | 6b |
| `whatsapp-rust/src/send/mod.rs:836` | Always `<device-identity>` | 7 |
| `whatsapp-rust/src/send/mod.rs:843` | `session_scope="status"` | 8 |
| `waos/lib/src/media.rs:174` | `media_key_timestamp` in build_media_message | — |
| `whatsapp-rust/src/media.rs:21-42` | ImageOptions — height/width/thumbnail fields | 9 |
| `whatsapp-rust/src/media.rs:100-127` | image_message() — pass new fields to proto | 9 |
| `whatsapp-rust/src/media.rs:45-72` | VideoOptions — height/width/thumbnail/gif fields | 10 |
| `whatsapp-rust/src/media.rs:131-163` | video_message() — pass new fields to proto | 10 |
| `whatsapp-rust/src/features/status.rs:76-97` | send_image — media_opts param | 11 |
| `whatsapp-rust/src/features/status.rs:104-127` | send_video — media_opts param | 11 |
| `waos/lib/src/media.rs:30-100` | get_image_dimensions + probe_video_info helpers | — |
| `waos/lib/src/bridge/queue.rs:387-405` | send_status_image: dims + CDN thumbnail upload | — |
| `waos/lib/src/bridge/queue.rs:407-433` | send_status_video: dims+duration + CDN thumbnail upload | — |
| `waproto/src/whatsapp.desc` + `.sha256` | Regenerated from proto after DeviceListMetadata changes | — |
| `wacore/src/reporting_token.rs:662` | DeviceListMetadata in MessageContextInfo | 12 |
| `wacore/src/send/dm.rs:99` | Pass account to reporting_context_info | 12 |
| `wacore/src/send/group.rs:175` | Pass account to reporting_context_info | 12 |

