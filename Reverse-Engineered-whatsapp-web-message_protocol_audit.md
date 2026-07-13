# Message Sending Protocol Audit — WA Web vs Rust Implementation

**Verification status**: Each finding is labeled `[VERIFIED]` (confirmed from JS bundles),
`[SPECULATIVE]` (inferred but unproven), or `[CORRECTED]` (earlier version was wrong).

---

## 1. Protobuf Field Definitions `[VERIFIED]`

The `WAWebProtobufsE2E.pb` module (`rsrc_full/.../1bafe1ff7d2964b28b3b48ae.js`) defines every
wire field for Whatsapp protobufs. These are the **schema** — they define what the server
**can** accept, NOT necessarily what WA Web **does** send.

### 1.1 ImageMessage + VideoMessage — fields defined in schema but not set by Rust

| Field # | Name | In Rust? | Notes |
|---------|------|----------|-------|
| 6 | `height` | ❌ | Defined in ImageMessage schema |
| 7 | `width` | ❌ | Defined in ImageMessage schema |
| 9 | `height` | ❌ | Defined in VideoMessage schema (different field #) |
| 10 | `width` | ❌ | Defined in VideoMessage schema |

These are set by WA Web's `generateImageMessageProto`/`generateVideoMessageProto`.
The server **may** validate their presence, but we cannot confirm without a packet capture.

### 1.2 DeviceListMetadata — schema definition only

```protobuf
message MessageContextInfo {
  optional DeviceListMetadata deviceListMetadata = 1;   // defined in schema
  optional int32 deviceListMetadataVersion = 2;
  optional bytes messageSecret = 3;
  optional int32 reportingTokenVersion = 8;
  repeated ThreadID threadId = 15;
}

message DeviceListMetadata {
  optional bytes senderKeyHash = 1;
  optional uint64 senderTimestamp = 2;
  repeated uint32 senderKeyIndexes = 3 [packed = true];
  optional ADVEncryptionType senderAccountType = 4;
  optional ADVEncryptionType receiverAccountType = 5;
  optional bytes recipientKeyHash = 8;
  optional uint64 recipientTimestamp = 9;
  repeated uint32 recipientKeyIndexes = 10 [packed = true];
}
```

Rust's `reporting_context_info` (`wacore/src/reporting_token.rs:662-668`) only sets:
- `message_secret` (field 3) ✅
- `reporting_token_version` (field 8) ✅

Missing: `device_list_metadata`, `device_list_metadata_version`, `thread_id`.

### 1.3 What we DON'T know `[UNVERIFIED]`

- Whether WA Web actually populates `deviceListMetadata` on outgoing messages
  (JS code was NOT found — may be C++/WASM)
- Whether the server **requires** this field or accepts messages without it
- What values would be correct (senderKeyHash = SHA-256 of accountSignatureKey?
  senderTimestamp from ADVDeviceIdentity? senderKeyIndexes from ADVKeyIndexList?)

---

## 2. Verified Outgoing Stanza Structure `[VERIFIED]`

Found in `WAWebSendMsgCreateFanoutStanza.createFanoutMsgStanza()` in
`rsrc_full/.../QYtmU9dW-Td.js`.

### 2.1 Full Outgoing Stanza (multi-device fanout)

```xml
<message to="{jid}" id="{id}" type="text|media|reaction|...">
  <participants>
    <to jid="{device_jid}">
      <enc v="2" type="pkmsg|msg" mediatype="image|..." decrypt-fail="hide">
        [ciphertext bytes]
      </enc>
      <!-- NO device-identity -->
      <!-- NO device-identity anywhere in this stanza -->
    </to>
    <!-- more <to> nodes for each recipient device -->
  </participants>
  <!-- optional nodes -->
</message>
```

### 2.2 Single-Device Shortcut

For 1:1 sends where the recipient has one device, the `<participants><to>` wrapper is
skipped and `<enc>` goes directly under `<message>`:

```xml
<message to="{jid}" id="{id}" type="text">
  <enc v="2" type="msg" mediatype="image">
    [ciphertext bytes]
  </enc>
</message>
```

### 2.3 Key attributes on `<enc>`

| Attribute | Source | Values |
|-----------|--------|--------|
| `v` | Hardcoded | `"2"` |
| `type` | Encryption result | `"pkmsg"`, `"msg"` |
| `mediatype` | `mediaTypeFromProtobuf(message)` | `"image"`, `"video"`, `"ptt"`, etc. |
| `decrypt-fail` | Conditional | `"hide"` for certain edits/revokes |
| `state` | Conditional | `"false"` for some pkmsg cases |

---

## 3. `<device-identity>` — NOT in Outgoing Message Stanzas `[VERIFIED]`

### 3.1 The Incoming Parser

WA Web's `WAWebHandleMsgParser` reads `<device-identity>` as a **sibling of `<enc>`**
at the **message level** (not inside `<participants><to>`) from incoming server messages.

### 3.2 Outgoing — NOT Found

Searched 889 JS files. Zero matches for `<device-identity>` in any outgoing message
stanza builder. It only exists in:

- **Pairing responses** (`<pair-device-sign>`, `<hosted-pair-set>`) — registration flow
- **Incoming message parsing** — server/sender sends it to the client
- **Retry request parsing** — incoming receipts
- **Pre-key bundle parsing** — incoming device lists
- **VOIP decryption validation** — incoming offers

### 3.3 Correction from Earlier Version

The earlier version of this doc claimed WA Web nests `<device-identity>` inside
`<participants><to>`. This was **wrong**. The incoming JS parser reads it at message
level. No outgoing JS code adds it to message stanzas at all.

### 3.4 What This Means for Rust

Rust's `needs_device_identity()` in `wacore/src/send/encrypt.rs` adds `<device-identity>`
at message level when a PKMSG is emitted. This MAY be:

- **Correct** — the WASM/C++ layer in WA Web handles this identically (invisible to JS)
- **Unnecessary** — the PKMSG ciphertext embeds the sender identity internally
- **Incorrect** — the server may reject or ignore it

**Cannot determine from JS bundles alone.** A wire capture comparing WA Web vs Rust
would be needed.

---

## 4. Stanza Type Constants `[VERIFIED]`

WA Web's `WASmax*Mixin` modules define stanza type strings that match Rust's constants:

| Constant | WA Web | Rust (`wacore/src/send.rs`) |
|----------|--------|---------------------------|
| `"text"` | ✅ | ✅ |
| `"media"` | ✅ | ✅ |
| `"reaction"` | ✅ | ✅ |
| `"poll"` | ✅ | ✅ |
| `"event"` | ✅ | ✅ |

---

## 5. Addressing Mode — `[UNVERIFIED]`

Rust adds `addressing_mode="0"` or `addressing_mode="1"` on group message stanzas.
WA Web's outgoing stanza structure in JS was **not found to include this attribute**,
but this could be in the WASM layer or a different code path.

---

## 6. Phash — `[UNVERIFIED]`

Rust sends `phash` attribute on group messages only when SKDM is distributed.
WA Web's phash behavior in outgoing stanzas could not be fully verified from JS
(phash computation exists, but the send-time attribute logic wasn't found).

---

## 7. JS Source References

The analysis uses the bundle at `web.whatsapp.com-module-bundle/rsrc_full/`:

| Module | Bundle Path | Content |
|--------|-------------|---------|
| `WAWebProtobufsE2E.pb` | `.../1bafe1ff7d2964b28b3b48ae.js` | Full protobuf spec for all message types |
| `WAWebSendMsgCreateFanoutStanza` | `.../QYtmU9dW-Td.js` (line 844) | **Outgoing** stanza builder for fanout messages |
| `WAWebSendMsgCreateDeviceStanza` | `.../QYtmU9dW-Td.js` (line 871) | Device-specific stanza builder |
| `WAWebE2EProtoGenerator` | Multiple files | `generateImageMessageProto()`, `generateVideoMessageProto()` |
| `WAWebHandleMsgParser` | Multiple files | **Incoming** message parser (reads `<device-identity>`) |
| `WASmax*Mixin` | `.../67fec6ec384359e1b2472637.js` | Stanza type constants |
| `WAWebRetryRequestParser` | `.../cItmOMC1aCZBPA...js` (line 530) | **Incoming** retry parser (reads `<device-identity>`) |

---

## 8. Summary — What's Actually Actionable

| Finding | Status | Action? |
|---------|--------|---------|
| `height`/`width` fields exist in schema, not in Rust | `[VERIFIED]` schema | Probable gap if server validates |
| `DeviceListMetadata` fields exist in schema, not in Rust | `[VERIFIED]` schema | Unknown if required (WASM layer) |
| `messageAssociation` exists in schema | `[VERIFIED]` schema | Unknown if sent |
| `<device-identity>` placement | `[CORRECTED]` — wrong earlier | Rust's current placement at message level matches incoming parser. No outgoing JS code found to compare. |
| No `<device-identity>` in outgoing JS stanzas | `[VERIFIED]` | Either WASM handles it or Rust should not add it |
| Stanza type constants match | `[VERIFIED]` | ✅ OK |
| `addressing_mode` attribute | `[UNVERIFIED]` | Cannot confirm WA Web behavior |
| `phash` always present on groups | `[UNVERIFIED]` | Cannot confirm WA Web behavior |
