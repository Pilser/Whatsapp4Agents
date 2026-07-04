# WhatsApp Status Protocol — Reverse-Engineered

Comprehensive document detailing how WhatsApp Web handles **Status** (formerly "Status", now "Updates" tab), extracted from a live WhatsApp Web JS bundle snapshot (June 2026, version 2.3000.x).

Source: `WhatsApp_Business_resources.zip` → `/tmp/wa-resources/`

---

## Table of Contents

1. [Architecture Overview](#1-architecture-overview)
2. [Status Data Model](#2-status-data-model)
3. [Sending Status](#3-sending-status)
4. [Receiving Status](#4-receiving-status)
5. [Status Privacy](#5-status-privacy)
6. [Status Expiry (24h)](#6-status-expiry-24h)
7. [Status Viewing & Read Receipts](#7-status-viewing--read-receipts)
8. [Status Interactions (Reactions, Replies, Reshares)](#8-status-interactions)
9. [WAM Telemetry Events](#9-wam-telemetry-events)
10. [Storage & Persistence](#10-storage--persistence)
11. [Text Status ("About") vs Media Status](#11-text-status-about-vs-media-status)
12. [Protocol Wire Format](#12-protocol-wire-format)
13. [Complete Module Map](#13-complete-module-map)
14. [Key Enums](#14-key-enums)

---

## 1. Architecture Overview

Status in WhatsApp Web operates as a **sender-key encrypted broadcast message** to a dynamic list of recipients. Key architectural points:

- **Status JID**: `STATUS_JID` constant from `WAJids` — a special JID used as the chat target
- **Encryption**: Uses **Sender Key** (SKMSG) distribution, same pattern as group messages
- **Fan-out**: Messages are encrypted once with sender key, then distributed to all recipients
- **Recipient list**: Dynamically computed from privacy settings (`My Contacts`, `My Contacts Except...`, `Only Share With...`)
- **Expiry**: Automatic after 24 hours (`86400` seconds constant)
- **Stanza tag**: Normally `"message"`; can be `"status"` via gating flag

### Status Types

| Type | Description | Protocol |
|------|-------------|----------|
| **Media Status** | Photo, Video, GIF, Audio | E2E encrypted `message` stanza to STATUS_JID with SenderKey |
| **Text Status** | Plain text status update | Same as media but with text protobuf |
| **Status Interaction** | Reactions, Replies, Mentions | `message` stanza with `reaction` or `protocol` type |
| **Text Status ("About")** | The "About" line on profile | MEX notifications (`xwa2_notify_text_status_on_update`) |
| **PSA Status** | Public Service Announcements | Special campaign messages from STATUS_JID |

---

## 2. Status Data Model

### StatusCollection (`WAWebStatusCollection`)

A **collection** (extends `BaseCollection`) keyed by **contact JID**. Each entry is a `StatusModel`.

```typescript
class StatusCollection extends BaseCollection<StatusModel> {
  model = StatusModel;
  comparator = (a, b) => b.t - a.t; // newest first

  // Primary query
  findImpl(id): Promise<StatusModel>;
  findQueryImpl(): Promise<StatusModel[]>; // calls WAWebContactStatusBridge.queryStatusAll()

  sync(): Promise<StatusModel[]>; // fetches all statuses, logs WAM metrics

  // Status Model access
  getStatusModel(jid, isRevoke?): StatusModel | null;

  // Incoming message handler
  handleUpdate(msg, isUpdate?): boolean;
  // Returns true if the message was accepted as a status

  addStatusMessages(contactJid, msgs[]): Promise<void>;

  // Filtering
  getValidStatus(): StatusModel[]; // non-expired, non-muted visible statuses
  getUnexpired(hasUnread?): StatusModel[];
  getMyStatus(): StatusModel | null;
  getPSAStatus(): StatusModel | null;
}
```

### StatusModel (`WAWebStatusModel`)

```typescript
class StatusModel extends BaseModel {
  id: Jid;           // The contact's JID
  t: number;         // Unix timestamp of latest status
  unreadCount: number; // default 0
  totalCount: number;  // default 0
  pic: ?string;      // base64 JPEG thumbnail (extracted from first msg body)
  statusMsgs: Msg[]; // Array<Msg> (populated from DB queries)
  isLoading: boolean; // session-only
  isSyntheticFromMetadata: boolean; // session-only (newsletter last_status_sent_time)
  readKeys: Set<string>; // session-only (msg key IDs that have been read)
  contact: Contact;  // session-only, resolved via ContactCollection
  expireTimer: ?Timer;
  expireTs: ?number;

  isExpired(): boolean;        // 24h check
  isUnreadMessage(msgId): boolean;
  revokeMsgs(msgIds[]): void;
  expireMsg(): void;           // purge expired messages from this model
  sendReadStatus(contact, msgs, isFromMe): Promise<boolean>;
  containsMessage(msgKey): boolean;
  handleReadStatus(msgKey): void; // marks key as read, decrements unreadCount
  onEmptyMRM(): void;          // if all msgs removed, delete model
}
```

### StatusModel Properties

- **`id`**: JID of the contact who posted
- **`t`**: Unix timestamp of the latest status
- **`unreadCount`**: Number of unread status items
- **`totalCount`**: Total number of status items
- **`pic`**: Thumbnail extracted from first message's body (base64 JPEG)
- **`statusMsgs`**: The actual message models behind the status
- **`isSyntheticFromMetadata`**: Flag for newsletter synthetic status (created from `lastStatusSentTime` metadata without actual messages)
- **`expireTs`**: Timestamp used for scheduling expiry

---

## 3. Sending Status

### Flow

```
sendStatusTextMsgAction(text)
  or
sendStatusMediaMsgAction(msg)
    │
    ▼
create status Msg data (WAWebStatusMsgDataUtils)
    │
    ▼
storeMsg locally (DB)
    │
    ▼
add to StatusCollection.handleUpdate()
    │
    ▼
addStatusMessages() — process into STATUS_JID chat
    │
    ▼
waitForPrep() (media: wait for upload)
    │
    ▼
WAWebEncryptAndSendStatusMsg.encryptAndSendStatusMsg()
    │
    ├─► getStatusList() — fetch privacy-based recipient list
    ├─► getFanOutList() — device-level fanout
    ├─► createOrMergeReceiptRecords() — pre-create receipt tracking
    ├─► ensureE2ESessions() — ensure sessions exist
    ├─► genMessageBody() — encrypt with SenderKey
    └─► send stanza → await ack
```

### Encrypt and Send (`WAWebEncryptAndSendStatusMsg`)

```typescript
async function encryptAndSendStatusMsg(sendMsgRecord, msgProtobuf, metricsReporter) {
  const STATUS_JID = createWid(STATUS_JID);
  const meLid = getMeDeviceLid();
  const sessionScope = shouldUseStatusSession()
    ? SessionScope.STATUS
    : SessionScope.DEFAULT;

  // Get privacy-based recipient list
  const statusList = await getStatusList();
  // Convert to LIDs
  const senderList = statusList.list.map(toUserLid);

  // Check if revoke
  const revokeMsgKey = extractRevokeKey(msgProtobuf);
  if (revokeMsgKey) {
    // Direct revoke (sender key distribution only to affected)
    const revokeList = await calculateRevokeSenderList(revokeMsgKey);
    if (hasNonPrivacyRevoke(revokeList, senderList)) {
      // Send as direct SKMSG to revoke list
      const deviceList = await getFanOutList({ wids: [...revokeList, meLid] });
      return encryptAndSendStatusDirectMsg(deviceList, msgProtobuf, sendMsgRecord, sessionScope);
    }
    // Otherwise send as regular status message
  }

  // Build device list (including self)
  const deviceList = await getFanOutList({ wids: [...senderList, meLid] });

  // Get sender key distribution list
  const { participantList, skDistribList } = await getStatusSkDistribList(deviceList);

  // Create receipt records before sending
  await createOrMergeReceiptRecords(
    deviceList.map(id => ({ msgKey: msgData.id, receiverId: id }))
  );

  // Distribute sender key
  if (skDistribList.length > 0) {
    await ensureE2ESessions(skDistribList, false, sessionScope);
  }

  // Encrypt message body
  const [participantsNode, encNode, deviceIdentityNode] = await genMessageBody(
    STATUS_JID, meLid, senderKeyDistributionList,
    participantList, msgProtobuf, sessionScope
  );

  // Build stanza
  const metaNode = wap("meta", {
    status_setting: mapPrivacySetting(statusList.setting),
    session_scope: sessionScope === SessionScope.STATUS ? "status" : undefined
  });

  const stanza = wap("message", {
    id: msgData.id,
    to: CHAT_JID(STATUS_JID),
    type: typeFromProtobuf(msgProtobuf),
  }, participantsNode, encNode, deviceIdentityNode, metaNode, reportingToken);

  // Apply status tag (if gated)
  maybeApplyStatusTag(stanza); // may change <message> to <status>

  // Send
  await sendStanza(stanza, ackTemplate);
  await markStatusHasSenderKey(skDistribList);
}
```

### Stanza Format (Sending)

```xml
<!-- When isStatusStanzaSendEnabled() is FALSE (current default): -->
<message id="..." to="status@broadcast" type="text|image|video|etc">
  <participants>
    <to jid="user@wid" />
    <to jid="user@lid" />
    <!-- Sender Key Distribution Messages for new devices -->
    <to jid="device@jid">
      <enc v="2" type="pkmsg">ciphertext_b64</enc>
    </to>
  </participants>
  <enc v="2" type="skmsg" mediatype="...">sender_key_ciphertext_b64</enc>
  <device-identity>adv_encoded_identity</device-identity>
  <meta status_setting="contacts|allowlist|denylist" session_scope="status" />
</message>

<!-- When isStatusStanzaSendEnabled() is TRUE: -->
<status id="..." to="status@broadcast" type="text|image|video|etc">
  <!-- same children -->
</status>
```

**Important**: The `<meta>` node contains the `status_setting` attribute which tells the server the privacy setting:
- `"contacts"` → My Contacts
- `"allowlist"` → Only Share With
- `"denylist"` → My Contacts Except

This is how the server knows who to deliver the status to.

### Sender Key Distribution

Status uses **SenderKey** (SKMSG), same as groups. The sender key is distributed via:

1. **Pre-key messages (PKMSG)** sent as `<to>` nodes under `<participants>` — target devices that don't have the sender key yet
2. **Same ciphertext** is sent to all devices (the SKMSG-encrypted payload)
3. Receipt records are pre-created for all recipient devices

---

## 4. Receiving Status

### How Status Messages Arrive

Status messages arrive as **regular message stanzas** addressed from `STATUS_JID`:

```xml
<message id="..." from="status@broadcast" type="text|image|video" data="...">
  <enc v="2" type="skmsg" mediatype="...">ciphertext</enc>
  <participants>
    <to jid="user@wid" />
  </participants>
</message>
```

The **receiving flow**:

1. **Normal message pipeline** processes the stanza (like any group message with SenderKey)
2. `StatusCollection.handleUpdate(msg)` is called which:
   - Detects `from = STATUS_JID` or `id.remote.isStatus()`
   - Creates/finds the `StatusModel` for the sender (extracted from `author` attribute)
   - Increments `totalCount` and `unreadCount`
   - If first message in status, extracts `body` as base64 JPEG for thumbnail
   - If revoke, decrements counts

### Receiving Path (from `v4iy2x4` bundles)

```typescript
// In WAWebStatusCollection.handleUpdate(msg, isUpdate):
handleUpdate(msg, isUpdate) {
  // Find sender JID
  const a = getSender(msg) ?? msg.from;
  if (msg.id.remote.isStatus()) {
    a = msg.id.fromMe ? matWidConvert(meUser) : msg.author;
  }

  if (this.hasSynced()) {
    if (isUpdate) return true;

    const isRevoke = msg.subtype === "sender_revoke" || msg.subtype === "admin_revoke";
    const model = this.getStatusModel(a, isRevoke);

    if (model) {
      if (!isRevoke) {
        // New status message
        model.set({
          totalCount: model.totalCount + 1,
          t: msg.t
        });
        if (msg.invis !== true) {
          model.set({ unreadCount: model.unreadCount + 1 });
        }
        // Extract thumbnail from first msg
        if (body is non-empty) {
          createFromBase64Jpeg(msg.body).then(pic => model.pic = pic);
        }
      } else {
        // Revoke: remove message from model
        model.revokeMsgs([...]);
      }
    } else if (!isRevoke && a) {
      // Create new model
      this.add({ id: a, t: msg.t, unreadCount: 1, totalCount: 1 });
    }
  }
}
```

### Text Status ("About") — Separate Path

Text status (profile "About" line) uses a completely different mechanism:
1. **MEX notifications**: `xwa2_notify_text_status_on_update`
2. Handled by `WAWebMexTextStatusUpdateNotificationHandler`
3. Results in `Contact.setAbout()` on the contact model
4. Also fetched via `WAWebContactStatusBridge.getStatus(wid)` which queries `WAWebMexUsersGetAboutStatus`

---

## 5. Status Privacy

### Privacy Setting Types (`WAWebUserPrefsStatusType`)

```typescript
enum StatusPrivacySettingType {
  Contact = "contact",    // My Contacts
  AllowList = "allow-list", // Only Share With...
  DenyList = "deny-list",   // My Contacts Except...
}
```

### Syncd Privacy Sync (`WAWebStatusPrivacySettingSync`)

Uses **AppState sync** (Syncd) to sync privacy settings across devices:

```typescript
class StatusPrivacySettingSync extends AccountSyncdActionBase {
  collection = CollectionName.RegularHigh;
  action = Actions.StatusPrivacy;
  version = 7;

  applyMutations(mutations[]) {
    // Parses StatusPrivacyAction protobuf
    // Maps mode to StatusDistributionMode enum:
    //   CONTACTS, ALLOW_LIST, DENY_LIST, CLOSE_FRIENDS, CUSTOM_LIST
    // Writes to IndexedDB
    // Triggers BackendEventBus update
  }

  getStatusPrivacySettingMutation(setting, userJid, timestamp, shareToFB, shareToIG) {
    // Builds SyncdMutation with SyncdOperation.SET
  }
}
```

### Setting and Syncing (`WAWebStatusSetAndSyncPrivacy`)

```typescript
async function setAndSyncStatusPrivacy({ setting, list, overrideShareToFB, overrideShareToIG }) {
  const mutation = StatusPrivacySettingSync.getStatusPrivacySettingMutation(
    setting,
    list.map(jid => jid.toString({ legacy: true })),
    unixTimeMs(),
    shareToFB, shareToIG
  );
  await lockForSync(["user-prefs"], [mutation], async () => {
    // Persist crosspost settings
    // Write status privacy config locally
    setStatusPrivacyConfig({ setting, list });
  });
}
```

### Status List Computation

The recipient list is built dynamically per-send:
- `getStatusList()` — returns `{ setting, list }` based on user's current privacy config
- `getStatusContacts()` — all contacts (used for `contacts` mode)
- `getStatusAllowList()` — explicitly allowed JIDs
- `getStatusDenyList()` — explicitly denied JIDs
- Contact JIDs are converted via `WAWebStatusPrivacyContactsUtils.convertPrivacyListContactsToWids()` which maps usernames to LIDs

---

## 6. Status Expiry (24h)

### Constants

```typescript
const STATUS_EXPIRY_SECONDS = 86400; // 24 hours
const STATUS_EXPIRY_MS = 86400 * 1000;
```

### Expiry Mechanism (`WAWebStatusModel`)

Each `StatusModel` manages its own expiry:

```typescript
// Called on model initialization and after every add
setupStatusExpiration() {
  const firstMsg = this.msgs.at(0);
  if (!firstMsg) return;
  const scheduledTime = firstMsg.t + EXPIRY_SECONDS + 1;
  if (scheduledTime < now) {
    setTimeout(() => this.expireMsg());
  } else {
    this.expireTimer = setGlobalTimeout(
      () => this.expireMsg(),
      scheduledTime * 1000,
      this.expireTimer
    );
    this.expireTs = firstMsg.t;
  }
}

// Called when timer fires
expireMsg() {
  AppTracker.start(AppTrackerType.PurgeStatuses);
  for (const msg of this.msgs) {
    if (msg.t + EXPIRY_SECONDS > now) break;
    this.removeMessage(msg); // removes from model + deletes
  }
  AppTracker.stop(AppTrackerType.PurgeStatuses);
  // Reschedule if still has messages
  if (this.totalCount > 0) this.setupStatusExpiration();
}
```

### PSA Expiry

PSA (Public Service Announcement) statuses expire based on:
- `campaignDuration` - campaign-specific duration
- `statusPSAReadTimestamp` - after first read + duration
- Or falls back to 24h default

---

## 7. Status Viewing & Read Receipts

### Navigation (StatusSnapshot)

`WAWebStatusSnapshot` creates an iterator over statuses for the viewer:

```typescript
class StatusSnapshot {
  statuses: Array<{ status, totalCount, unreadCount, msgs[], readMsgKeys }>;

  constructor(model, isFromPog, specificMsgKey?) {
    if (specificMsgKey) {
      // Single message view
    } else if (isFromPog && model.unreadCount > 0 && !isMe && !isMuted) {
      // Unread only (POG tap)
      statuses = getUnexpired({ containsAnyUnreadStatus: true })
    } else {
      // Full status for this contact
    }
  }

  getFirstUnread(model, isForward, specificMsgKey?): Promise<Index>;
  getNext(index): Promise<Index>;
  getPrev(index): Promise<Index>;
  hasNext(index): boolean;
  hasPrev(index): boolean;
  statusAt(index, msgIdx): Promise<Index>;
}
```

### Read Status

```typescript
// In StatusModel
handleReadStatus(msgKey) {
  const key = msgKey.toString();
  if (!this.readKeys[key]) {
    this.readKeys[key] = true;
    if (this.unreadCount > 0) this.unreadCount--;
  }
}

async sendReadStatus(contact, msgs, isFromMe) {
  // Delegates to WAWebSendReadReceiptJob.markStatusRead()
  // Triggers receipt stanza
  await markStatusRead(contact, msgs, isFromMe);
  this.handleReadStatus(msgs[0].id);
  return true;
}
```

### Receipt Handling (`WAWebHandleStatusReceipt`)

```typescript
async handleStatusSimpleReceipt({
  ack, ackString, externalIds, offline, participant, recipient, ts
}) {
  // Extract msgKey from externalIds[0]
  // Build STATUS_JID as remote
  const msgKey = new MsgKey({ id, remote: STATUS_JID, fromMe: !isMe, participant });

  // Queue for batch processing
  if (isFromMe) {
    // Self receipt (my status was viewed by someone)
    await receiptBatcher.acceptPeerReceipt({ ts, msgKeys, ack, isOffline, remote: STATUS_JID });
  } else {
    // Other receipt (someone viewed my status)
    await receiptBatcher.acceptOtherReceipt({ ack, ts, receiverId, msgKeys });
  }

  // Batch flush (if not offline)
  if (!offline) receiptBatcher.runActiveBatches();
}
```

---

## 8. Status Interactions

### Reactions

Status reactions are sent as **reaction messages** to `STATUS_JID`:

```typescript
async function sendStatusReaction(statusMsg, emoji) {
  const me = matWidConvert(meUser);
  const msgKey = new MsgKey({
    fromMe: true,
    remote: STATUS_JID,
    id: await newId(),
    participant: asUserWid(me)
  });
  const msgData = await buildStatusReactionMsgData({ msgKey, reaction: emoji, parent: statusMsg });
  // Encrypt and send directly (only to self + poster)
  const deviceList = await getFanOutList({ wids: [poster, me] });
  const protobuf = createOutgoingMessageProtobuf(originType, { data: msgData });
  await encryptAndSendStatusDirectMsg({ deviceList, msgProtobuf: protobuf, sessionScope });
}
```

### Reply to Status

Users can reply to a status via `WAWebOpenStatusQuotedFlow`, which opens a modal showing the status and allows composing a reply message. This is not a "status reply" — it creates a **regular 1-on-1 chat message** quoting the status.

### Reshare Status

Status resharing (`WAWebStatusReshareAction`) allows reposting someone else's status. Uses a `WAWebAddonConstants` flow.

---

## 9. WAM Telemetry Events

### Status-Specific WAM Events

| Event Name | ID | Description |
|---|---|---|
| `StatusPost` | 1176 | Status posted (sent). Includes 60+ fields |
| `StatusPostImpression` | 6364 | Status post viewed in viewer |
| `StatusPogImpression` | 8302 | Status POG (ring) seen in updates tab |
| `StatusItemView` | 1658 | Individual status item viewed |
| `StatusRowView` | 1656 | Status row seen in scrolling view |
| `StatusPosterActions` | 3546 | Poster-side actions (creation, editing, deletion) |
| `StatusInteractionReceived` | 6810 | Received reaction/reply/etc |
| `StatusInteractionSent` | 6812 | Sent reaction/reply/etc |
| `StatusReplyWamEvent` | — | Status reply |
| `StatusRevokeWamEvent` | — | Status revoke |
| `StatusMuteWamEvent` | — | Status mute action |
| `StatusPrivacySettings` | 3200 | Privacy settings changes |
| `StatusViewerActionWamEvent` | — | Viewer actions |
| `StatusCrosspostRequestWamEvent` | — | Crosspost to FB/IG |
| `StatusReportingEventsWamEvent` | — | Reporting |
| `WebcStatusSessionWamEvent` | — | Session metrics |
| `WebcStatusSyncWamEvent` | — | Sync timing |

### Key WAM Event: StatusPost (ID 1176)

```typescript
// Fields available in StatusPostWamEvent
{
  statusSessionId: INT,          // session counter
  mediaType: MEDIA_TYPE,         // PHOTO, VIDEO, etc
  statusPostOrigin: INT,         // WHERE the post was initiated from
  statusPostResult: INT,         // OK, ERROR_*, CANCELLED
  retryCount: INT,
  defaultStatusPrivacySetting: INT, // privacy enum
  perPostStatusPrivacySetting: INT, // per-post override
  statusAudienceSelectorClicked: BOOL,
  statusAudienceSelectorUpdated: BOOL,
  statusMentionCount: INT,
  statusAudienceSize: INT,
  isReshare: BOOL,
  isResultTerminal: BOOL,
  // ... 50+ more fields
}
```

### StatusPosterActions Event (ID 3546)

Tracks every significant user action during the status creation flow:
- Camera actions, Gallery taps, Text tool usage
- Drawing, Music, Stickers, Filters
- Media cropping, trimming
- Privacy changes per-post
- Post success/failure at each stage

---

## 10. Storage & Persistence

### Status Messages Storage

Status messages are stored in the **same message DB** as regular messages:
- `WAWebDBProcessMessage.storeMessages()` — saves to IndexedDB
- `WAWebDBMessageDelete.removeStatusMessage()` — removes on revoke
- Messages have `remote = STATUS_JID`, `author = sender`

### Status Receipt Tracking

Receipt records are stored in `WAWebApiMessageInfoStore`:
- `createOrMergeReceiptRecords()` — bulk insert of `{ msgKey, receiverId }` pairs
- `updatePeerStatusReceiptInfo()` — batched (5s delay via `WABatcher`)

### User Preferences

`WAWebUserPrefsStatus` stores in IndexedDB:
- `StatusPrivacySetting` — current privacy config (setting type + list)
- `StatusAllowList` — allow list JIDs
- `StatusDenyList` — deny list JIDs
- `StatusSenderKeyMap` — maps recipient JID to sender key info
- `ShareToFB` / `ShareToIG` — crosspost settings
- `StatusReshareAllowed` — whether resharing is permitted

---

## 11. Text Status ("About") vs Media Status

### Two Separate Systems

| Feature | Media Status | Text Status ("About") |
|---------|-------------|----------------------|
| Content | Image, Video, GIF, Audio, Text | Pure text (profile about) |
| Duration | 24 hours | Until changed |
| Encryption | E2E (SenderKey) | Not E2E |
| Delivery | Message stanza to STATUS_JID | MEX notification |
| Privacy | Per-status config | Per-contact query |
| Update mechanism | Send message → revoke | `setMyStatus()` → MEX |

### Media Status (Status Tab)

The main "Status" feature (now "Updates" tab) that shows your contacts' stories.

### Text Status (About Line)

The text shown on profile pages. Queried on-demand via:
- `WAWebContactStatusBridge.getStatus(wid)` — fetches about text
- MEX notification handler updates the contact model when changed

---

## 12. Protocol Wire Format

### Sending a Status Message (Binary Protocol Node)

```
<message id="<id>" to="status@broadcast" type="image" t="<ts>">
  <participants>
    <to jid="<user>@s.whatsapp.net" />
    <to jid="<user>@lid" />
    <to jid="<device>">
      <enc v="2" type="pkmsg"><ciphertext_b64></enc>
    </to>
  </participants>
  <enc v="2" type="skmsg" mediatype="image/jpeg"><sender_key_ciphertext_b64></enc>
  <device-identity><adv_b64></device-identity>
  <meta status_setting="contacts" session_scope="status" />
</message>
```

### Receiving a Status Message (Binary Protocol Node)

```
<message id="<id>" from="status@broadcast" type="image" t="<ts>">
  <enc v="2" type="skmsg" mediatype="image/jpeg"><ciphertext_b64></enc>
  <participants>
    <to jid="<sender>@s.whatsapp.net" />
  </participants>
</message>
```

### Receipt for Status

```
<receipt to="<sender>@s.whatsapp.net" type="read" id="<msg_id>">
</receipt>
```

### Status Tag Rewriting

When the gating flag `status_e2ee_send_over_status_stanza` is enabled, the tag changes from `"message"` to `"status"`:

```javascript
// WAWebEncryptAndSendStatusMsg.maybeApplyStatusTag()
function maybeApplyStatusTag(stanza) {
  if (isStatusStanzaSendEnabled()) {
    stanza.tag = "status";  // Rewrite <message> → <status>
    return "status";
  }
  return "message";
}
```

---

## 13. Complete Module Map

### Core Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `WAWebStatusCollection` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Central collection for status models |
| `WAWebStatusModel` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Status data model (per-contact) |
| `WAWebStatusGetters` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Computed properties (getHasUnread, getReadCount) |
| `WAWebStatusSnapshot` | v4iTQd4/y_/l/en_GB-j/xOAEHIgRDT9.js | Viewer navigation iterator |
| `WAWebStatusUtils` | All bundles | Status message ack labels |
| `WAWebStatusGatingUtils` | All bundles | Feature flags for status features |
| `WAWebStatusJidUtils` | (embedded in other modules) | Status JID utilities |

### Sending Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `WAWebEncryptAndSendStatusMsg` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | E2E encrypt + send status stanza |
| `WAWebSendStatusMsgAction` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Orchestrates sending (text + media) |
| `WAWebRevokeStatusAction` | v4iTQd4/y_/l/en_GB-j/xOAEHIgRDT9.js | Status revoke flow |
| `WAWebDeleteStatusAction` | v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js | Cleanup removed contacts' statuses |
| `WAWebStatusMsgDataUtils` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Create status message data objects |
| `WAWebMediaMsgUtils` | (dispersed) | Media-specific status handling |
| `WAWebPostSendStatusFailure` | All bundles | Handle post-failure for status |

### Receiving/Bridge Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `WAWebContactStatusBridge` | v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js | Main bridge: getStatus, queryStatusAll, sendReadStatus |
| `WAWebStatusBridgeApi` | v4izPN4/y9/l/en_GB-j/cItm...js | Bridge API: handleUpdate, handleReadStatus, fillGap |
| `WAWebHandleStatusReceipt` | v4iuqx4/yn/l/en_GB-j/YLZrMeJhCGT.js | Process incoming status receipts |
| `WAWebStatusVisibilityUtils` | (embedded) | Status visibility/expiry |

### Privacy/Sync Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `WAWebUserPrefsStatus` | v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js | Status user preferences (persistence) |
| `WAWebUserPrefsStatusType` | All bundles | Privacy setting type enums |
| `WAWebStatusPrivacySettingAction` | v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js | Action wrappers for privacy setting changes |
| `WAWebStatusPrivacySettingSync` | All bundles | Syncd sync for privacy |
| `WAWebStatusSetAndSyncPrivacy` | All bundles | Set + sync privacy settings |
| `WAWebStatusSetAndSyncMute` | All bundles | Set + sync status mute |
| `WAWebStatusPrivacyContactsUtils` | v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js | Convert privacy list contacts to WIDs |
| `WAWebUserStatusMuteSync` | All bundles | Syncd sync for status mute |

### Logging/Telemetry Modules

| Module | Location | Purpose |
|--------|----------|---------|
| `WAWebLogStatusPost` | v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js | Log status post WAM event |
| `WAWebLogStatusPosterActions` | All bundles | Log poster actions WAM event |
| `WAWebLogStatusPogImpression` | All bundles | Log POG impression |
| `WAWebLogStatusReply` | All bundles | Log reply |
| `WAWebLogStatusRevoke` | All bundles | Log revoke |
| `WAWebLogStatusPrivacySettings` | All bundles | Log privacy settings |
| `WAWebStatusLoggingUtils` | All bundles | Shared logging utilities |

### Newsletter Status Modules

| Module | Purpose |
|--------|---------|
| `WAWebNewsletterStatusApiParse` | Parse newsletter status from API |
| `WAWebNewsletterStatusGapFillAction` | Fill gaps in subscribed newsletter status |
| `WAWebNewsletterSendStatusJob` | Send newsletter status |
| `WAWebNewsletterRevokeStatusAction` | Revoke newsletter status |
| `WAWebNewsletterSyntheticStatusUtils` | Create synthetic status entries from newsletter metadata |
| `WAWebHandleNewsletterStatus` | Process incoming newsletter status stanzas |

---

## 14. Key Enums

### WAM Enum: StatusCategory

```typescript
enum STATUS_CATEGORY {
  REGULAR_STATUS = 1,   // Normal user status
  GROUP_STATUS = 2,     // Group status
  CHANNEL_STATUS = 3,   // Channel status
  ENGAGEMENT_CARD = 4,  // Engagement/promotional card
}
```

### WAM Enum: StatusContentType

```typescript
enum STATUS_CONTENT_TYPE {
  PHOTO = 1,
  TEXT = 2,
  URL = 3,
  VIDEO = 4,
  GIF = 5,
  VOICE = 6,
  FUTURE = 7,
  PLACEHOLDER = 8,
}
```

### WAM Enum: StatusType

```typescript
enum STATUS_TYPE {
  IMAGE = 1,
  VIDEO = 2,
  GIF = 3,
  AUDIO = 4,
  TEXT = 5,
  MUSIC_STANDALONE = 6,
  FUTURE = 7,
  PLACEHOLDER = 8,
  INLINE_VIDEO = 9,
}
```

### WAM Enum: StatusPostResult

```typescript
enum STATUS_POST_RESULT {
  OK = 1,
  CANCELLED = 2,
  ERROR_UNKNOWN = 3,
  MEDIA_ERROR_REQUEST = 4,
  MEDIA_ERROR_UPLOAD = 5,
  // ... 45+ error variants
}
```

### WAM Enum: StatusPrivacyType

```typescript
enum STATUS_PRIVACY_TYPE {
  ALL_CONTACTS = 1,
  EXCEPT = 2,
  ONLY_WITH = 3,
  CLOSE_FRIENDS = 4,
  CUSTOM_LIST = 5,
}
```

### WAM Enum: StatusRowSection

```typescript
enum STATUS_ROW_SECTION {
  RECENT_STORIES = 1,
  PREVIOUS_STORIES = 2,
  MUTED_STORIES = 3,
  MY_STATUS = 4,
  CHAT_LIST = 5,
  // ... 35+ sections
}
```

### WAM Enum: StatusViewEntryMethod

```typescript
enum STATUS_VIEW_ENTRY_METHOD {
  FORWARD_TAP = 1,
  BACK_TAP = 2,
  FORWARD_SWIPE = 3,
  BACK_SWIPE = 4,
  DIRECT_POG_TAP = 5,
  AUTO_NAVIGATE_TIMER_END = 6,
  STATUS_WIDGET = 7,
  CHAINING_PILL_TAP = 8,
  ENGAGEMENT_CARD_CTA_TAP = 9,
  MY_STATUSES_SCREEN = 10,
}
```

### WAM Enum: StatusViewExitMethod

```typescript
enum STATUS_VIEW_EXIT_METHOD {
  UNKNOWN = 0,
  SWIPE_DOWN = 1,
  BACK_ARROW_TAP = 2,
  BACK_BUTTON_TAP = 3,
  STATUS_TIMEOUT = 4,
  STATUS_DISMISSED = 5,
  BACKWARD_SWIPE = 6,
  // ...
}
```

### Protobuf: StatusDistributionMode (SyncAction)

```typescript
enum StatusDistributionMode {
  CONTACTS = 0,
  ALLOW_LIST = 1,
  DENY_LIST = 2,
  CLOSE_FRIENDS = 3,
  CUSTOM_LIST = 4,
}
```

### WAM Enum: StatusPosterContactType

```typescript
enum STATUS_POSTER_CONTACT_TYPE {
  UNKNOWN = 0,
  CONTACT = 1,
  TRUSTED_INDIVIDUAL = 2,
  TRUSTED_GROUP_MEMBER = 3,
  SELF = 4,
  CHANNEL = 5,
}
```
