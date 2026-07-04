# Status Sending Protocol — WhatsApp Web Reverse Engineering

Based on JS bundle analysis of WhatsApp Web (extracted from live runtime).

---

## 1. Overview

Media statuses (image, video, GIF, audio, PTT) use E2E encryption (SenderKey/skmsg) fanned out to `status@broadcast`. Key properties:

- **Target JID**: `status@broadcast`
- **Encryption**: E2E SenderKey (skmsg), identical to group message encryption
- **Lifetime**: 24h (86400s constant in `WAWebStatusCollection`)
- **Delivery**: Fan-out to dynamic recipient list from privacy settings
- **Privacy modes**: `contacts`, `allowlist`, `denylist` — synced via Syncd AppState
- **Stanza tag**: `"message"` (default) or `"status"` when gated by `status_e2ee_send_over_status_stanza`
- **Protobuf**: Standard `Message` (image, video, audio, etc.)

---

## 2. Message Types & Controller Dispatch

On send, `WAWebStatusSetupController.setupStatusController(mediaData, msg)` selects the view controller that drives playback duration:

| Media Type | Controller | Duration Logic |
|------------|-----------|----------------|
| IMAGE | `WAWebStatusCountdownController` | 4500ms + 2000ms per ~150 chars of caption |
| VIDEO (non-GIF) | `WAWebStatusBaseMediaController` (MediaType.Video) | Actual video duration |
| VIDEO (GIF) | `WAWebStatusGifController` | 3 loops of video + 6s countdown |
| AUDIO / PTT | `WAWebStatusBaseMediaController` (MediaType.Audio) | Actual audio duration |
| Text (MSG_TYPE.CHAT) | `WAWebStatusCountdownController` | 2000ms + 60ms/char (max 1000 chars) |
| Default | `WAWebStatusCountdownController` | 3000ms |

The controllers are timer/view-layer only — they do NOT affect wire protocol.

---

## 3. Required Fields Per Media Type

In the `SendStatusMsgAction._sendStatusMessage()` pipeline, the protobuf is generated via `WAWebE2EProtoGenerator.getProtobufMessage()`. Fields are set on the message data object, which is then serialized to protobuf.

### 3.1 IMAGE

```js
{
  type: "image",
  kind: MsgKind.Image,
  mediaData: {
    // — Encryption / upload fields —
    mediaBlob: MediaOpaqueData,        // encrypted file blob (opaque ref-counted wrapper)
    mediaKey: Uint8Array(32),          // AES-256 key for media decryption
    mediaKeyTimestamp: number,         // unix seconds when mediaKey was generated
    directPath: string,                // server path to uploaded encrypted media
    encFilehash: Uint8Array(32),       // SHA-256 of the encrypted file
    filehash: Uint8Array(32),          // SHA-256 of the plaintext file
    fileLength: number,                // plaintext file size in bytes
    mimetype: "image/jpeg"|"image/png",
    height: number,
    width: number,

    // — Progressive JPEG thumbnail (preferred for images) —
    scanLengths: number[],             // byte lengths of each progressive scan
    scansSidecar: Uint8Array,          // progressive scan sidecar data (enables fast low-res decode)
    // OR MMS thumbnail fallback:
    thumbnailDirectPath: string,       // server path to uploaded thumbnail
    thumbnailSha256: Uint8Array(32),   // SHA-256 of the plaintext thumbnail
    thumbnailEncSha256: Uint8Array(32),// SHA-256 of the encrypted thumbnail blob
    thumbnailHeight: number,
    thumbnailWidth: number,
    thumbnailLength: number,
    thumbnailMimetype: string
  },
  caption: string | undefined,         // optional text caption
  isViewOnce: boolean | undefined,     // view-once media
  messageSecret: Uint8Array(32)|null   // reporting token (if gated)
}
```

### 3.2 VIDEO

```js
{
  type: "video",
  kind: MsgKind.Video,
  mediaData: {
    // — Encryption / upload fields (same as IMAGE) —
    mediaBlob, mediaKey, mediaKeyTimestamp,
    directPath, encFilehash, filehash,
    fileLength, mimetype: "video/mp4",
    height, width,

    seconds: number,                   // duration in seconds
    streamingSidecar: Uint8Array,      // MP4 sidx/moof boxes for streaming

    // — Video thumbnail (frame capture) —
    thumbnailDirectPath: string,
    thumbnailSha256: Uint8Array(32),
    thumbnailEncSha256: Uint8Array(32),
    thumbnailHeight: number,
    thumbnailWidth: number,
    thumbnailLength: number,
    thumbnailMimetype: "image/jpeg"
  },
  caption: string | undefined,
  isGif: boolean,                      // true for GIF-type videos
  isViewOnce: boolean | undefined,
  gifAttribution: number | undefined,  // GIF source attribution
  messageSecret: Uint8Array(32)|null
}
```

### 3.3 GIF

GIF uses the same structure as VIDEO with `isGif: true`. The server treats GIFs and videos differently for thumbnail generation and streaming. The `WAWebStatusGifController` loops the video 3 times then shows a 6s countdown before advancing.

### 3.4 AUDIO

```js
{
  type: "audio",
  kind: MsgKind.Audio,
  mediaData: {
    mediaBlob, mediaKey, mediaKeyTimestamp,
    directPath, encFilehash, filehash,
    mimetype: "audio/ogg; codecs=opus",
    fileLength,
    seconds: number,                   // duration in seconds
    waveform: Uint8Array | undefined   // 64-byte waveform for PTT visualization
  }
}
```

Audio statuses have **no thumbnail**. The only visual is a waveform (for PTT) or a static audio icon.

### 3.5 PTT (Push-to-Talk / Voice Note)

```js
{
  type: "ptt",
  kind: MsgKind.Ptt,
  mediaData: {
    mediaBlob, mediaKey, mediaKeyTimestamp,
    directPath, encFilehash, filehash,
    mimetype: "audio/ogg; codecs=opus",
    fileLength,
    seconds: number,
    waveform: Uint8Array | undefined   // 64-byte waveform
  }
}
```

PTT also has **no thumbnail**. It displays a waveform animation during playback.

### 3.6 STICKER

```js
{
  type: "sticker",
  kind: MsgKind.Sticker,
  mediaData: {
    mediaBlob, mediaKey, mediaKeyTimestamp,
    directPath, encFilehash, filehash,
    mimetype: "image/webp",
    fileLength,
    height: number,
    width: number,
    isAnimated: boolean,
    pngThumbnail: Uint8Array | undefined   // static PNG fallback for animated stickers
  },
  stickerSent: boolean
}
```

Stickers are small enough (typically <100KB) that they often **do not need a separate thumbnail**. The `pngThumbnail` field is used only for animated stickers as a static fallback.

---

## 4. Thumbnail Requirements — Deep Dive

### 4.1 Which Media Types Need Thumbnails

| Media Type | Thumbnail Required? | Preferred Method | Fallback |
|-----------|-------------------|-----------------|----------|
| IMAGE | **YES** | Progressive JPEG (`scanLengths` + `scansSidecar`) | MMS thumbnail download |
| VIDEO | **YES** | Video frame capture (`downloadVideoThumbnail`) | MMS thumbnail download |
| GIF | **YES** | Same as VIDEO (frame capture) | MMS thumbnail download |
| AUDIO | **NO** | — | — |
| PTT | **NO** | — | — |
| STICKER | **NO** | — | — |
| Text (chat type) | **NO** | — | — |

Only image and video types need thumbnails on status messages. The thumbnail selector (`WAWebStatusMediaMsgUtils.findStatusMediaMsg`) searches for `["image", "video", "ptv"]` to attach thumbnail data to the status viewer.

### 4.2 IMAGE Thumbnail — Progressive JPEG (pJPEG)

**Preferred path.** The client encodes the image as a progressive JPEG and extracts scan boundaries:

```
Fields required:
- scanLengths: number[]  — byte offset of each progressive scan start
- scansSidecar: Uint8Array — progressive scan sidecar data
```

**How progressive JPEG works:**
- The JPEG is encoded with multiple scans (typically 3-5)
- Scan 1 = very low quality (fast decode, tiny bytes)
- Scan 2+ = incremental quality improvements
- The `scanLengths` array tells the decoder where each scan starts
- `scanCount` parameter: normally 1 (only first scan for preview), 2 if `isUpdatesTabInNavigationBarEnabled()` gate is active

**Client behavior (`WAWebDownloadStatusThumbnail`):**
```js
if (msg.type === IMAGE && msg.scanLengths && msg.scansSidecar) {
  await downloadProgressiveJpegThumbnail({
    msg, signal, isPreload,
    scanCount: isUpdatesTabEnabled ? 2 : 1
  });
  return; // success, no fallback needed
}
```

The `scanCount` determines how many progressive scans to use:
- **1 scan** = very rough preview (fastest)
- **2 scans** = better quality (used when updates tab shows larger thumbnails)

### 4.3 VIDEO Thumbnail — Frame Capture

**Preferred path for videos (non-GIF).** The server or client extracts a frame from the video:

```js
if (msg.type === VIDEO && msg.isGif !== true) {
  await downloadVideoThumbnail({ msg });
  return;
}
```

The `downloadVideoThumbnail` function either:
1. Uses a pre-generated thumbnail from the upload response, OR
2. Extracts a frame at ~1s position from the video blob

### 4.4 MMS Thumbnail Fallback

**Fallback for both IMAGE and VIDEO** when the preferred method is unavailable:

```js
// Condition:
if (thumbnailSha256 && thumbnailDirectPath &&
    (thumbnailEncSha256 || isNewsletterMsg)) {
  await WAWebMediaDownloadMmsThumbnail({
    msg, signal, isPreload, chat: null
  });
}
```

**Required fields for MMS fallback:**
```
thumbnailDirectPath: string        — server URL to the uploaded thumbnail blob
thumbnailSha256: Uint8Array(32)     — SHA-256 hash of the thumbnail (for integrity)
thumbnailEncSha256: Uint8Array(32)  — SHA-256 of the encrypted thumbnail (only needed if media crypto is expected)
thumbnailHeight: number             — thumbnail pixel height
thumbnailWidth: number              — thumbnail pixel width
thumbnailLength: number             — thumbnail file size in bytes
thumbnailMimetype: string           — typically "image/jpeg"
```

### 4.5 Upload Flow for Thumbnails

```
1. Media upload:
   a. Encrypt media with mediaKey → encrypted blob
   b. Upload encrypted blob → server returns {directPath, mediaKeyTimestamp, encFilehash}
   
2. Thumbnail generation (depends on media type):
   For IMAGE:
     a. Create progressive JPEG version of the image
     b. Extract scanLengths[] and scansSidecar from pJPEG
     c. Optionally upload a separate MMS thumbnail → get thumbnailDirectPath
   
   For VIDEO:
     a. Extract a frame at ~1s as JPEG
     b. Upload the thumbnail → get thumbnailDirectPath, thumbnailSha256, thumbnailEncSha256

3. Assemble message data with all fields
4. Call sendStatusMediaMsgAction(msg, sendPreparedMediaFn)
```

### 4.6 Thumbnail Quality Levels

The hook `useWAWebThumbnail` determines the best quality thumbnail URL via `WAWebMediaDataUtils.getHighestQualityThumbnailUrl(mediaData)`. The priority order is:

1. **Streaming/progressive URL** (scan-based) — highest quality, from `scansSidecar`
2. **Direct thumbnail path** (`thumbnailDirectPath`) — medium quality
3. **Inline jpegThumbnail** (protobuf field) — lowest quality, embedded in message

### 4.7 Protobuf Thumbnail Fields

Each media message protobuf type carries these thumbnail fields:

| Protobuf Message | Field # | Field Name | Type |
|-----------------|---------|-----------|------|
| `ImageMessage` | 16 | `jpegThumbnail` | bytes (inline) |
| `ImageMessage` | 19 | `thumbnailDirectPath` | string |
| `ImageMessage` | 20 | `thumbnailSha256` | bytes |
| `ImageMessage` | 21 | `thumbnailEncSha256` | bytes |
| `ImageMessage` | 24 | `thumbnailHeight` | uint32 |
| `ImageMessage` | 25 | `thumbnailWidth` | uint32 |
| `VideoMessage` | 16 | `jpegThumbnail` | bytes (inline) |
| `VideoMessage` | 19 | `thumbnailDirectPath` | string |
| `VideoMessage` | 20 | `thumbnailSha256` | bytes |
| `VideoMessage` | 21 | `thumbnailEncSha256` | bytes |
| `VideoMessage` | 24 | `thumbnailHeight` | uint32 |
| `VideoMessage` | 25 | `thumbnailWidth` | uint32 |

The `jpegThumbnail` inline bytes contain the full (or reduced-quality) JPEG. The `thumbnailDirectPath` points to a server-hosted version. For images with `scanLengths`/`scansSidecar`, the thumbnail is rendered from the progressive scan data rather than from these fields.

---

## 5. Media Upload Flow

### 5.1 Standard Send

```
1. Determine privacy list → WAWebUserPrefsStatus.getStatusList()
2. Upload media:
   a. Encrypt media with mediaKey (AES-256-CBC or HMAC-then-encrypt)
   b. Upload encrypted blob → server returns {directPath, mediaKeyTimestamp}
   c. Compute encFilehash (SHA-256 of encrypted blob)
   d. For images: encode progressive JPEG, extract scanLengths/scansSidecar
   e. For videos: extract frame thumbnail, upload separately
3. Build message data with all fields
4. Call sendStatusMediaMsgAction(msg, sendPreparedMediaFn)
```

### 5.2 Reshare Flow

`WAWebPrepareMediaForStatusForwarding.prepareMediaForStatusForwarding(msg)`:

```
1. Download original media if not already cached (may be expensive — downloads even if large)
2. Strip stale upload fields: delete directPath, mediaKey, mediaKeyTimestamp, encFilehash, mediaStage
3. Restore mediaBlob from InMemoryMediaBlobCache if available (keyed by filehash)
4. Create MediaOpaqueData wrapper (reference-counted blob wrapper)
5. Return mediaData.toJSON() for new upload
```

The original media must be RE-UPLOADED because the old `directPath`, `mediaKey`, and `encFilehash` are tied to the original sender's upload.

### 5.3 MediaOpaqueData

`WAWebMediaOpaqueData` is a reference-counted wrapper similar to `CFRetain`/`CFRelease`:

```
retain()       — increment refcount
autorelease()  — decrement refcount (batch release ~1s via WAShiftTimer)
url()          — get blob URL (prefers base64 JPEG data, then createObjectURL)
getBlob()      — get underlying Blob
getBase64()    — get base64 if created from string
createFromData(data, mimetype)        — from Blob/ArrayBuffer
createFromBase64Jpeg(base64)          — from base64 string
createScaledImageIfLarger(img, w, h)  — downscale image
```

---

## 6. Stanza Construction

The wire stanza is built by `WAWebEncryptAndSendStatusMsg.encryptAndSendStatusMsg()`:

```xml
<!-- Normal send (tag = "message") -->
<message id="{id}" to="status@broadcast" type="text">
  <participants>
    <participant jid="{recipient1}" />
    <participant jid="{recipient2}" />
    <!-- ... fan-out list from privacy settings -->
  </participants>
  <enc type="skmsg" v="{version}">
    {encrypted_payload}
  </enc>
  <device-identity>
    {signature_certificate_chain}
  </device-identity>
  <meta status_setting="{contacts|allowlist|denylist}" />
  <!-- If session_scope is STATUS: -->
  <meta session_scope="status" />
</message>

<!-- If status stanza gating enabled via isStatusStanzaSendEnabled(): -->
<status id="{id}" to="status@broadcast" type="text">
  <!-- same child elements -->
</status>
```

**Privacy setting mapping** (`WAWebEncryptAndSendStatusMsg`):
```
StatusPrivacySetting → stanza meta value:
  "contacts"  → "contacts"
  "allowlist" → "allowlist"
  "denylist"  → "denylist"
```

**Key children:**
- `<participants>` — server-side delivery list (the server fans out only to these JIDs)
- `<enc type="skmsg">` — SenderKey encrypted message payload
- `<device-identity>` — sender's signed device identity certificate chain
- `<meta status_setting="...">` — tells server which privacy list was used
- `<meta session_scope="status">` — signals that this message uses the status session scope (separate from chat sessions)

---

## 7. Encryption Pipeline

```
1. Get status privacy list → WAWebUserPrefsStatus.getStatusList()
2. Fan-out devices → WAWebDBDeviceListFanout.getFanOutList(statusList)
   - Converts status recipients into per-device fan-out
3. Compute SenderKey distribution list → getStatusSkDistribList()
   - Finds devices that lack the sender key
   - Those devices get PKMSG (PreKey message) distribution messages
4. Ensure E2E sessions → ensureE2ESessions(destinations)
   - Establishes Signal sessions with devices that don't have one
   - May trigger prekey fetch if needed
5. Encrypt body:
   a. Serialize protobuf message to bytes
   b. Encrypt with Signal sender-key cipher → skmsg ciphertext
   c. Include sender-key distribution messages for new devices
6. Build stanza (see §6)
7. Send via deprecatedSendStanzaAndReturnAck()
8. Pre-create receipt records:
   WAWebApiMessageInfoStore.createOrMergeReceiptRecords()
   → Creates placeholder receipt records for all recipients before the message is sent
```

**For direct sends** (reactions, revokes): `encryptAndSendStatusDirectMsg()` uses per-user Signal sessions instead of SenderKey. Direct sends target only specific users (poster + self), not the full privacy list.

---

## 8. Revocation

`WAWebRevokeStatusAction.sendStatusRevokeMsgAction(targetMsg, msgStore)`:

1. `WAWebEncryptAndSendStatusMsg.calculateRevokeSenderList(msgKey)`
   - Retrieves the original recipient list from the stored send record
   - Returns all JIDs that received the original status
2. Creates a `MSG_TYPE.PROTOCOL` / `MsgKind.ProtocolRevoke` message:
   ```js
   {
     type: "protocol",
     kind: MsgKind.ProtocolRevoke,
     subtype: "sender_revoke",
     protocolMsgKey: targetMsg.id,  // the original status message key
     revokeDuration: now - originalMsg.t  // time elapsed since send
   }
   ```
3. Calls `encryptAndSendStatusMsg()` with revoke flag → direct encrypt to each recipient (NOT SenderKey broadcast)
4. On success: `WAWebDBMessageDelete.removeStatusMessage()` — removes from local IndexedDB

---

## 9. Status Interactions

### 9.1 Status Reactions — Like (💚)

The "Like" reaction is a single emoji: **💚 (Green Heart)** = `\u{1F49A}`.

**Sending flow** (`WAWebSendStatusMsgAction.sendStatusReaction()`):

```
1. buildStatusReactionMsgData({ msgKey, parent, reaction: "💚" })
   → Creates REACTION type message targeting STATUS_JID
   → reactionTimestamp = dedup key (unique per reaction)
2. processOutgoingMsgOptimisticUpdate() — updates local UI immediately
3. encryptAndSendStatusDirectMsg() — encrypts + sends to:
   - The poster (original status author)
   - The sender (self, for cross-device sync)
4. WAWebReactionsUtils.updateRecentReaction("💚", timestamp)
```

**Reaction message structure:**
```js
{
  type: "reaction",
  kind: MsgKind.Reaction,
  id: MsgKey { remote: STATUS_JID, participant: authorLid, ... },
  parentMsgKey: MsgKey,              // original status message key
  reactionText: "💚",
  reactionTimestamp: timestamp,       // dedup key
  fromMe: true,
  to: STATUS_JID,
  author: meUser
}
```

**Key differences from chat reactions:**
- NOT broadcast to all viewers — direct message to poster + self only
- Uses `encryptAndSendStatusDirectMsg` (per-user Signal session) not SenderKey
- Same protobuf type (`MSG_TYPE.REACTION`) as regular message reactions
- Participant field on MsgKey distinguishes which user's status was reacted to

**Like counting** (`WAWebStatusLikeEmojis`):
- `countLikes(reactionCountMap: Map<EmojiAggregate, number>)` — counts only 💚 across a reaction map
- `countStatusLikes(reactionModels)` — counts reaction models matching 💚 aggregate

### 9.2 Status Quick Reply — Emoji Bar

The status viewer shows a quick reply bar. Emojis are configured via AB `"status_reaction_emojis"`:

| Default (8) | Allowed (26 total) |
|-------------|-------------------|
| 😍 😂 😮 😢 🙏 👏 🎉 💯 | + 👍 ❤️ 😊 🥰 🤩 💚 🤣 😆 🔥 🙌 🥳 😭 😔 🩷 😲 🤯 😡 😤 |

**Click behavior** (`WAWebStatusComposeBox.handleQuickReplySend`):
```
1. Closes status viewer
2. Opens 1-on-1 chat with the poster
3. Sends the emoji as a TEXT message to the poster (private reply)
4. Logs: StatusReplyWamEvent with replyType = QUICK_REPLY
```

This is NOT a reaction — it's a private chat message sent as a reply. The 💚 goes through the reaction path (§9.1), while all other emojis go through the reply path (§9.3).

### 9.3 Status Reply (Private Text/Sticker)

Users can type a reply or send a sticker in `WAWebStatusComposeBox`:

```
1. User types reply text (or selects sticker)
2. WAWebStatusComposeBox.sendTextMsgToChat(text, quotedMsg)
   → quotedMsg = the original status message
3. Opens 1-on-1 chat with the poster
4. Sends the reply as a REGULAR chat message with the status as quoted message
5. If sticker: WAWebStickerPicker → sendStickerToChat() with quoted status
6. Logs result: WAWebLogStatusReply logs StatusReplyWamEvent (ID 1180)
```

**Reply types** (StatusReplyMessageType):
- `TEXT` — regular text reply
- `STICKER` — sticker reply
- `QUICK_REPLY` — quick emoji tap

**WAM Event fields** (`WAWebStatusReplyWamEvent`, ID 1180, 27 fields):
- `replyEntryMethod` — how user entered reply (bar tap, emoji, text input)
- `replyExitMethod` — how user exited (sent, cancelled, error)
- `replyResult` — OK, CANCELLED, ERROR_UNKNOWN
- `replyMessageType` — TEXT, STICKER, QUICK_REPLY
- `isRecentQuickReplyUsed`, `postContainedPrompt`, `statusContentType`
- `mediaHeight`, `mediaWidth`, `isReplyBarBelowCanvas`, `isReplyBarOverMedia`

### 9.4 Read Receipts

Batched (5s via `WAWebDataBatcher`) and privacy-gated:

```
WAWebHandleStatusReceipt.handleStatusSimpleReceipt(receiptType, participant, msgKey)
  → Dispatches to WAWebMessageReceiptBatcher:
    - If from me → acceptPeerReceipt (someone viewed my status)
    - If from other → acceptOtherReceipt (I viewed someone's status)
```

Read receipt privacy is checked: if user has "none" and receipt is not READ from self, it's skipped.

### 9.5 Viewing / Impression Events

| Event | ID | Trigger |
|-------|----|---------|
| `StatusRowView` | 1656 | Status row appeared in contact list |
| `StatusItemView` | 1658 | Individual status message viewed |
| `StatusPostImpression` | 6364 | Full status post impression logged |
| `StatusPogImpression` | 8302 | Profile picture impression in status viewer |

### 9.6 Interaction WAM Events

| Event | ID | Direction | Key Fields |
|-------|----|-----------|------------|
| `StatusInteractionSent` | 6812 | Outgoing | `statusInteractionType` (REACTION), `statusInteractionMessageType` (LIKE), `statusInteractionActors` |
| `StatusInteractionReceived` | 6810 | Incoming | Same structure, for reactions received on own status |

**Interaction Type enum** (StatusInteractionType):
- `REACTION`
- Possibly `REPLY` (not confirmed in extracted enums)

**Actor enum** (StatusInteractionActors):
- `POSTER_VIEWER` — current user (most common)
- `SELF` — own action
- `POSTER` — the status poster

**Message Type enum** (StatusInteractionMessageType):
- `LIKE` — 💚 reaction

**Result Type enum** (StatusInteractionResultType):
- `OK`
- Other error types

The received-reaction logger (`WAWebStatusReactionCTLLogging.logReceivedStatusReactionEngagement`) processes reactions on status messages, logging `statusReactionsReceived` activity and committing a `StatusInteractionReceivedWamEvent`.

---

## 10. Privacy (Syncd)

Privacy is synced via AppState Syncd with `Actions.StatusPrivacy`, version 7.

**Settings:**
- `contacts` — all contacts (default)
- `allowlist` — only selected contacts
- `denylist` — exclude selected contacts

**The privacy list is evaluated at send-time** from `WAWebUserPrefsStatus.getStatusList()`. The result is passed to the server via the `<meta status_setting="...">` stanza attribute. The server fans out only to the specified recipients.

**Muting:** `WAWebStatusSetAndSyncMute.setAndSyncStatusMute(wid, muted)`
- Separate from privacy — mutes statuses from a specific contact
- Stored server-side, synced to client

**Contact removal:** `WAWebDeleteStatusAction.clearStatusForRemovedContact()`
- Removes all statuses for contacts not in address book, not group members, not subscribed newsletters

**SenderKey session scope:** `WAWebStatusSessionGatingUtils.shouldUseStatusSessionForOutgoingMessage()` — when true, status messages use `SessionScope.STATUS` instead of the default session scope, keeping status encryption separate from chat encryption.

---

## 11. Resharing

Controlled by feature gates and per-user preference:

**Gating:**
- `isStatusResharePosterSideEnabled()` — AB `"wa_web_reshare_poster_side_enabled"`
- `isStatusReshareViewerFlowEnabled()` — AB `"wa_web_status_resharer_flow_enabled"`

**canBeReshared field:**
- Set on message creation via `WAWebUserPrefsStatus.getStatusReshareAllowed()`
- Newsletter statuses always `canBeReshared: true`

**StatusAttribution protobuf** (`StatusAttribution`, packed into `statusAttributions[]` in the message):

| Type | Value | Sub-message |
|------|-------|-------------|
| `RESHARE` | 1 | `StatusReshare{ source, metadata }` |
| `EXTERNAL_SHARE` | 2 | `ExternalShare{ actionUrl, source, duration }` |
| `MUSIC` | 3 | `Music{ authorName, songId, title, artistAttribution, isExplicit }` |
| `STATUS_MENTION` | 4 | — |
| `GROUP_STATUS` | 5 | `GroupStatus{ authorJid }` |
| `RL_ATTRIBUTION` | 6 | `RLAttribution{ source }` (Ray-Ban/Oakley Meta Glasses) |
| `AI_CREATED` | 7 | `AiCreatedAttribution{ source }` |
| `LAYOUTS` | 8 | — |
| `NEWSLETTER_STATUS` | 9 | — |
| `STATUS_CLOSE_SHARING` | 10 | — |
| `PAID_PARTNERSHIP` | 11 | — |

When any attribution has `type === RESHARE`, the protobuf `ContextInfo.statusAttributionType` is set to `RESHARED_FROM_POST`.

**Reshare sources** (`StatusReshare.Source`):
- `INTERNAL_RESHARE` (1)
- `MENTION_RESHARE` (2)
- `CHANNEL_RESHARE` (3)
- `FORWARD` (4)

**External share sources** (`ExternalShare.Source`):
- INSTAGRAM (1), FACEBOOK (2), MESSENGER (3), SPOTIFY (4), YOUTUBE (5), PINTEREST (6), THREADS (7), APPLE_MUSIC (8), SHARECHAT (9), GOOGLE_PHOTOS (10), SOUNDCLOUD (11), SHAZAM (12)

**Reshare preparation** (`WAWebPrepareMediaForStatusForwarding.prepareMediaForStatusForwarding()`):
- Strips `directPath`, `mediaKey`, `mediaKeyTimestamp`, `encFilehash`, `mediaStage` (all must be re-uploaded)
- Restores `mediaBlob` from `InMemoryMediaBlobCache` if available
- Returns `mediaData.toJSON()` for upload pipeline

---

## 12. Incoming Status Processing

### 12.1 Stanza Receive

Incoming status stanzas:
- `<message from="status@broadcast" type="text">` (default)
- `<status>` with same structure (if `isStatusStanzaReceiveEnabled()` gate is on)

**Parsing** (`WAWebHistorySyncHandleStatusMessages`):
1. Sort incoming by `messageTimestamp`
2. Determine author: `fromMe ? meUser : participant`
3. Type: `MESSAGE_TYPE.OTHER_STATUS`
4. Chat: `status@broadcast`
5. Decrypt with `CiphertextType.Skmsg`
6. Handle receipt records + reporting tokens
7. Store via `handleSingleMsg()` with origin `"historyStatusMessages"`

### 12.2 Expiry

`WAWebStatusCollection` uses a 24h constant (`86400` seconds):
- Messages with `t < now() - 86400` are filtered out by `getUnexpired()`
- Each `StatusModel` has per-contact message timers
- Sorted by `t` descending

### 12.3 Storage

IndexedDB table `"status"`:
```
status_uuid (unique index) | sender_user_jid | type | timestamp | state | is_archived | media_row_id | secret
```

In-memory `StatusCollection` aggregates by contact:
```
StatusModel = { id: Wid, msgs: Msg[], totalCount: number, unreadCount: number }
```

---

## 13. WAM Events Reference

| ID | Event Name | Trigger |
|----|-----------|---------|
| 1176 | `StatusPost` | Status sent (primary event, 60+ fields) |
| 1180 | `StatusReply` | User replied to/viewed a status |
| 1656 | `StatusRowView` | Status row appeared in contact list |
| 1658 | `StatusItemView` | Individual status item viewed |
| 3200 | (Status-related, not extracted) | — |
| 3546 | `StatusPosterActions` | Poster action (delete, mute, hide) |
| 6364 | `StatusPostImpression` | Full impression of a status post |
| 6810 | `StatusInteractionReceived` | Reaction received on own status |
| 6812 | `StatusInteractionSent` | Reaction sent to someone's status |
| 8302 | `StatusPogImpression` | Profile picture impression in status |
| — | `WebcStatusSync` | Status sync completed from DB |
| — | `WebcStatusSession` | Status viewer session lifecycle |

---

## 14. Implementor's Checklist

### Sending a Media Status

```
□ 1. Determine privacy list (contacts|allowlist|denylist) from UserPrefsStatus
□ 2. Upload encrypted media blob → get directPath, mediaKeyTimestamp
□ 3. Compute encFilehash, filehash, fileLength
□ 4. For IMAGE:
     □ Encode progressive JPEG
     □ Extract scanLengths[] + scansSidecar
     □ OR upload MMS thumbnail → get thumbnailDirectPath + hashes
□ 5. For VIDEO:
     □ Extract frame at ~1s as JPEG thumbnail
     □ Upload thumbnail → get thumbnailDirectPath + hashes
□ 6. Build MediaData object with ALL fields (see §3)
□ 7. Build message data with type, kind, mediaData, caption, etc.
□ 8. Build MsgKey with remote=STATUS_JID, participant=meLid
□ 9. Set canBeReshared from UserPrefsStatus
□ 10. Encrypt with Signal SenderKey → skmsg ciphertext
□ 11. Compute SenderKey distribution list
□ 12. Distribute SenderKey to devices that don't have it (PKMSG)
□ 13. Build stanza: <message|status> with
     □ <participants>
     □ <enc type="skmsg">
     □ <device-identity>
     □ <meta status_setting="...">
     □ <meta session_scope="status"> (if gated)
□ 14. Send via deprecatedSendStanzaAndReturnAck()
□ 15. Pre-create receipt records for all recipients
□ 16. On ack success → update local ack to SENT
□ 17. On failure → update to FAILED, post metric
```

### Sending a Reaction (💚)

```
□ 1. Build REACTION type MsgKey (remote=STATUS_JID, participant=posterLid)
□ 2. Build msg data: type="reaction", kind=MsgKind.Reaction
□ 3. Set parentMsgKey = original status MsgKey
□ 4. Set reactionText = "💚", reactionTimestamp = dedup key
□ 5. Direct encrypt (per-user Signal session, NOT SenderKey)
□ 6. Send to poster + self ONLY
□ 7. Update recent reactions list
```

### Revoking a Status

```
□ 1. calculateRevokeSenderList(originalMsgKey) → get original recipients
□ 2. Build PROTOCOL/ProtocolRevoke message with sender_revoke subtype
□ 3. Direct encrypt to each recipient
□ 4. Remove from local storage on success
```

### Replying to a Status

```
□ 1. Close status viewer
□ 2. Open 1-on-1 chat with the poster
□ 3. Send REGULAR chat message with original status as quotedMsg
□ 4. Reply types: TEXT, STICKER, or QUICK_REPLY (emoji tap)
□ 5. Log StatusReplyWamEvent (ID 1180) for telemetry
```
