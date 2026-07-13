# Message Send Protocol Gaps — Ownership split

Based on the protocol audit, categorised by which crate owns the fix.

---

## whatsapp-rust (library crate)

| # | Problem | Status | File(s) |
|---|---------|--------|---------|
| 1 | **`DeviceListMetadata` missing** from `MessageContextInfo` on ALL outbound messages. `reporting_context_info()` only sets `message_secret` + `reporting_token_version`. WA Web sets `senderKeyHash`, `senderTimestamp`, `senderKeyIndexes` from the device identity. | ❌ TODO | `wacore/src/reporting_token.rs:662` |
| 2 | **`device-identity` XML placement** — current code puts `<device-identity>` at message level (outside `<participants>`). WA Web nests it inside each `<to>` that used a pkmsg. Visible wire-level structural difference. | ❌ TODO | `wacore/src/send/dm.rs:196-204`, `wacore/src/send/group.rs:453-462` |
| 3 | **`addressing_mode` on group stanzas** — Rust sets `addressing_mode="1"` on group `<message>`. WA Web never sends this attribute for groups (only bare `<message>`). | ❌ TODO | `wacore/src/send/group.rs:516-518` |
| 4 | **`threadId: []` missing** from `MessageContextInfo`. WA Web always sets `threadId: []` (empty repeated field). | ❌ TODO | `wacore/src/reporting_token.rs:662` |
| 5 | **`messageAssociation` for status reactions** — WA Web attaches `STATUS_REACTION` association. Not implemented. | ❌ TODO | Needs new feature |
| 6 | **`phash` on ALL group sends** — WA Web always includes `phash` attribute even on warm sends. | ✅ DONE | `wacore/src/send/group.rs:313-332` |
| 7 | **`height`/`width`/`viewOnce`/`gifAttribution`** field definitions added to `ImageOptions`/`VideoOptions`. | ✅ DONE (structs) | `src/media.rs:22-73` |

## waos (application crate)

| # | Problem | Status | File(s) |
|---|---------|--------|---------|
| 1 | **Regular `build_media_message()`** — now accepts optional dimensions; sets `height`/`width` on ImageMessage and `height`/`width`/`seconds` on VideoMessage from the file. Thumbnail CDN fields (`thumbnailDirectPath`, `thumbnailSha256`, `thumbnailEncSha256`) remain unset for regular sends (status-only per WA Web). `jpeg_thumbnail` requires user-provided thumbnail file. | ✅ DONE (dimensions) | `waos/lib/src/media.rs:258-328` |
| 2 | **Status path** — `queue.rs` status sends already populate `height`/`width`/thumbnail CDN fields for status images/videos. | ✅ DONE (status only) | `waos/lib/src/bridge/queue.rs` |
| 3 | **Video duration detection** — `probe_video_info()` now used by regular `upload_and_send()` too. | ✅ DONE | `waos/lib/src/media.rs:200-250` |
| 4 | **Image dimension detection** — `get_image_dimensions()` now used by regular `upload_and_send()` too. | ✅ DONE | `waos/lib/src/media.rs:160-198` |

---

## Summary

**whatsapp-rust** owns the protobuf/wire-level issues (DeviceListMetadata, device-identity placement, addressing_mode, threadId) — these affect ALL messages (DM, group, status).

**waos** owns the media message content gaps (height/width/thumbnail CDN fields) in `build_media_message()` for regular DM/group media sends. Status media path is already complete.
