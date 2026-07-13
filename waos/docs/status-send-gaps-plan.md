# Status Send — Protocol Gaps Plan

## What Goes in whatsapp-rust/

### 1. ImageOptions — Add Fields (`src/media.rs`)

```
height: Option<u32>
width: Option<u32>
view_once: Option<bool>
thumbnail_direct_path: Option<String>
thumbnail_sha256: Option<Vec<u8>>
thumbnail_enc_sha256: Option<Vec<u8>>
```

### 2. VideoOptions — Add Fields (`src/media.rs`)

```
height: Option<u32>
width: Option<u32>
view_once: Option<bool>
gif_attribution: Option<i32>
thumbnail_direct_path: Option<String>
thumbnail_sha256: Option<Vec<u8>>
thumbnail_enc_sha256: Option<Vec<u8>>
```

### 3. image_message() Builder (`src/media.rs`)

Thread new fields into `wa::message::ImageMessage` protobuf fields.

### 4. video_message() Builder (`src/media.rs`)

Thread new fields into `wa::message::VideoMessage` protobuf fields.

### 5. status.rs — send_image/send_video Signatures (`src/features/status.rs`)

Add `media_opts: media::ImageOptions` / `media::VideoOptions` parameter so callers can pass dimensions + CDN thumbnail data.

### 6. StatusSendOptions — can_be_reshared (`src/features/status.rs`)

Add `can_be_reshared: bool` field. When true, inject `StatusAttribution { type: RESHARE }` into the message's context_info.

---

## What Goes in waos/

### 7. Extract Image Dimensions (`waos/lib/src/media.rs`)

Read JPEG/PNG headers to get height/width before building the message. Use `image` crate or minimal header parsing.

### 8. Extract Video Dimensions + Duration (`waos/lib/src/media.rs`)

Read MP4 headers or use ffprobe to get height/width and actual duration (replace hardcoded 30s).

### 9. Upload Thumbnail to CDN (`waos/lib/src/bridge/queue.rs`)

Currently passes inline `jpegThumbnail` only. Upload thumbnail file separately via `client.upload()`, then pass the resulting `thumbnailDirectPath`/`thumbnailSha256`/`thumbnailEncSha256` to `ImageOptions`/`VideoOptions`.

### 10. Pass New Fields in queue.rs (`waos/lib/src/bridge/queue.rs`)

- `send_status_image`: construct `ImageOptions` with dimensions + CDN thumbnail fields
- `send_status_video`: construct `VideoOptions` with dimensions + duration + CDN thumbnail fields

---

## Implementation Order

```
Step 1:  ImageOptions + VideoOptions struct fields    (whatsapp-rust)
Step 2:  image_message() + video_message() builders   (whatsapp-rust)
Step 3:  send_image/send_video signatures             (whatsapp-rust)
Step 4:  Dimension extraction + duration probing       (waos)
Step 5:  Thumbnail CDN upload                          (waos)
Step 6:  Wire everything in queue.rs                   (waos)
Step 7:  can_be_reshared (optional)                    (whatsapp-rust)
Step 8:  Update waos/docs/patched-whatsapp-rust.md      (tracking)
```
