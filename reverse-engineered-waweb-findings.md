# Reverse-Engineered WhatsApp Web Findings

Comprehensive document cataloging every fingerprinting signal, anti-detection mechanism, ban-related system, and behavioral expectation extracted from a live WhatsApp Web JS bundle snapshot (June 2026, version 2.3000.x, locale `en_GB-j`).

Source: `WhatsApp_Business_resources.zip` (443 MB), extracted to `/tmp/wa-resources/`.
Timestamp: 2026-06-28.

---

## Table of Contents

1. [ClientPayload & Noise Handshake](#1-clientpayload--noise-handshake)
2. [DeviceProps (Pairing Registration)](#2-deviceprops-pairing-registration)
3. [BrowserInfo & UA Detection](#3-browserinfo--ua-detection)
4. [Browser & Device Fingerprinting (WAM)](#4-browser--device-fingerprinting-wam)
5. [WAM Telemetry System](#5-wam-telemetry-system)
6. [Logout Reasons & Ban Codes](#6-logout-reasons--ban-codes)
7. [Anti-Spam / Reach-Out Timelock](#7-anti-spam--reach-out-timelock)
8. [Defense Mode / Brigading Protection](#8-defense-mode--brigading-protection)
9. [MQTT Connection Logging (Falco)](#9-mqtt-connection-logging-falco)
10. [Reconnect Backoff & Stability](#10-reconnect-backoff--stability)
11. [Syncd Device Fingerprint](#11-syncd-device-fingerprint)
12. [Presence & Typing Behavior](#12-presence--typing-behavior)
13. [Read Receipts](#13-read-receipts)
14. [Connect Attempt Count](#14-connect-attempt-count)
15. [Page Load Logging](#15-page-load-logging)
16. [Key Rotation & Device List Sync](#16-key-rotation--device-list-sync)
17. [Implications for Bot Clients](#17-implications-for-bot-clients)

---

## 1. ClientPayload & Noise Handshake

**Module:** `WAWebClientPayload`  
**File:** `v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js` (offset 445444, 7308 bytes)  
**Also in:** `v4iy2x4/ys/l/en_GB-j/3Z0q43jrKvT.js`, `v4iy2x4/yr/l/en_GB-j/4VKSvKqGKXv.js`, and all other locale-variant bundles.

### Three Entry Points

```js
__d("WAWebClientPayload",["WABase64","WAByteArray","WALogger","WAMd5",
"WAWebBackendApi","WAWebBrowserInfo","WAWebBuildConstants",
"WAWebClientFeatureFlags","WAWebClientPayloadReleaseChannel",
"WAWebEnvironment","WAWebLid1X1MigrationGating",
"WAWebProtobufsCompanionReg.pb","WAWebProtobufsWa6.pb",
"WAWebSyncdMdSyncFieldstatMeta","WAWebUserPrefsGeneral",
"WAWebUserPrefsMeUser","WAWebVoipGatingUtils","WAWebWamPlatform",
"asyncToGeneratorRuntime","encodeProtobuf","gkx","justknobx"],
(function(t,n,r,o,a,i,l){
  var e,s=null,u=0;
  function c(){u++}                     // incrementSuccessfulLoginCounter
  function d(e){...}                    // getClientPayloadForLogin
  function p(e,t,n){...}               // getClientPayloadForRegistration (pairing)
  function f(e){...}                    // base payload
  function h(){...}                     // buildHash = MD5(VERSION_BASE)
  function y(){...}                     // deviceProps
  function b(e){s=e}                   // setClientPayloadOverride
  function v(){...}                     // UserAgent builder
```

### getClientPayloadForLogin (`d`)
Sends:
- `username` — parsed from `getMeDevicePnOrThrow_DO_NOT_USE().user`
- `device` — from `getMeDevicePnOrThrow_DO_NOT_USE().device` (default 0)
- `lc` — login counter from `UserPrefsGeneral.getLoginCounter()`
- `connectAttemptCount` — persisted counter `u`, incremented per connect attempt
- `lidDbMigrated` — from LID migration gating
- All fields from base payload (`f`) + optional override `s`

### getClientPayloadForRegistration (`p`) — Pairing
Adds `devicePairingData`:
- `buildHash` — **MD5** of `VERSION_BASE` (NOT SHA-256!)
- `deviceProps` — encoded DeviceProps protobuf
- `eRegid` — `intToBytes(4, registrationId)`
- `eKeytype` — `intToBytes(1, 5)` (constant)
- `eIdent` — `identityKeyPair.pubKey`
- `eSkeyId` — `intToBytes(3, keyId)`
- `eSkeyVal` — `keyPair.pubKey`
- `eSkeySig` — `signature`

### Base Payload (`f`)
```js
{
  passive: e.passive ?? false,
  pull: e.pull ?? false,
  connectType: ClientPayload$ConnectType.WIFI_UNKNOWN,
  connectReason: ClientPayload$ConnectReason.USER_ACTIVATED,
  userAgent: (await v()),         // UserAgent builder
  webInfo: { webSubPlatform: L() }  // WEB vs DARWIN vs APP_STORE vs WIN_HYBRID
}
```

### buildHash (`h`)
```js
function h(){
  var e = decodeB64(md5(VERSION_BASE)),
      t = new Uint8Array(e);
  return t
}
```
**Important:** MD5, not SHA-256. `VERSION_BASE` is the version string without push-phase suffix.

### UserAgent Builder (`v`)
Fetches device info via `frontendSendAndReceive("getDeviceInfo")`, builds:
```js
{
  appVersion: {
    primary: Number(VERSION_PRIMARY),     // 2
    secondary: Number(VERSION_SECONDARY), // 3000
    tertiary: Number(VERSION_TERTIARY)    // client_revision
  },
  platform: ClientPayload$UserAgent$Platform.WEB,
  releaseChannel: (await getReleaseChannel()),
  mcc: e.mcc, mnc: e.mnc,
  osVersion: e.osVersion,           // from backend getDeviceInfo
  manufacturer: e.manufacturer,
  device: e.device,
  osBuildNumber: e.osBuild,
  localeLanguageIso6391: e.lg,      // "en"
  localeCountryIso31661Alpha2: e.lc // "GB"
}
```

**No `phone_id`** — the field exists in the protobuf spec but WA Web never populates it.

### setClientPayloadOverride (`b`)
Allows injecting extra fields into the payload (used for experiments).

---

## 2. DeviceProps (Pairing Registration)

**Module:** `WAWebClientPayload` (function `y` → `C`)  
**File:** Same as above.

### DeviceProps Construction
```js
function y(){return C.apply(this,arguments)}
function C(){return C=n("asyncToGeneratorRuntime").asyncToGenerator(function*(){
  var e = r("WAWebBrowserInfo")(), t;
  if(e.version && e.version !== ""){
    var n = e.version.split(".");
    n.length > 0 && e.version && /^[0-9\.]+$/.test(e.version) &&
      (t = {
        primary: parseInt(n[0], 10),
        secondary: n.length > 1 ? parseInt(n[1], 10) : undefined,
        tertiary: n.length > 2 ? parseInt(n[2], 10) : undefined
      })
  }
  var a = {
    inlineInitialPayloadInE2EeMsg: true,
    supportBotUserAgentChatHistory: true,
    supportCagReactionsAndPolls: true,
    supportRecentSyncChunkMessageCountTuning: true,
    supportHostedGroupMsg: true,
    supportBizHostedMsg: true,
    supportFbidBotChatHistory: true,
    supportMessageAssociation: true,
    supportCallLogHistory: isGroupCallingEnabled(),
    supportGroupHistory: gkx("15338"),
    thumbnailSyncDaysLimit: justknobx._("4736"),
    supportManusHistory: justknobx._("2304"),
    supportHatchHistory: justknobx._("2997"),
    supportedBotChannelFbids: []
  };
  // requireFullSync / storageQuotaMb logic...
  var u = isWindows ? DeviceProps$PlatformType.UWP : R(e.name);
  var c = {
    os: e.os,
    version: t,
    platformType: u,
    requireFullSync: i,
    historySyncConfig: a
  };
  return encodeProtobuf(DevicePropsSpec, c).readByteArrayView()
}),C.apply(this,arguments)}
```

### PlatformType Mapping (`R`)
```js
function R(name){
  // Chrome → Chrome(1), Firefox → Firefox(3), IE → IE(4),
  // Opera → Opera(5), Safari → Safari(6), Edge → Edge(2),
  // Electron → Desktop(7), UWP → UWP(8)
  // Windows always → UWP(8)
  // Default → OTHER_WEB_CLIENT(9)
}
```

### WebSubPlatform Mapping (`L`)
```js
function L(){
  // WEB / PWA → WEB_BROWSER
  // DARWIN → DARWIN
  // MACSTORE → APP_STORE
  // WIN_HYBRID → WIN_HYBRID
}
```

---

## 3. BrowserInfo & UA Detection

**Module:** `WAWebBrowserInfo`  
**File:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue-HJzVAh3RSGse3h5xtf34csUxJcJQJKdLotOCwd285yy_ZO3wMGMaCb4RI2I18ADx96ggj11-kKdiBfSpHuLKztW1IX_4BhnmWVUkOZOu41cTP7i9h1l_Qkm7cG6v5PXO7gtuca3BUhIVvwPoWOmYe_SWdbl7-qnLfcQGtacgyotkPlEdk6atr0ElIOSx6EE59iLcWTZJ04jpZbkPhrN_F.js` at offset ~4,689,021 (line 2014)

```js
__d("WAWebBrowserInfo",["WAWebUA"],(function(t,n,r,o,a,i,l){
  function e(){
    var e = s();
    return {
      os: e.os.name,          // "Mac OS", "Linux", "Windows"
      version: e.os.version,   // "10.15.7", "6.2.0"
      name: e.browser.name,    // "Chrome", "Firefox", "Edge", "Safari"
      ua: e.browser.name + " " + e.browser.version  // "Chrome 135"
    }
  }
  function s(){
    var e = o("WAWebUA").UA.parser.getResult();
    if(e != null && e.browser.name === "Chrome"){
      var t, n = self.navigator?.userAgentData?.brands;
      if(n != null){
        var r = n.some(function(e){ return e.brand === "Microsoft Edge" });
        r && (e.browser.name = "Edge")
      }
    }
    return e
  }
  l.default = e
}),98);
```

**Returns:** `{os, version, name, ua}`
- `os` = OS name from UA parser (e.g. "Mac OS", "Linux", "Windows")
- `version` = OS version from UA parser
- `name` = browser name with Edge-over-Chrome detection via `navigator.userAgentData.brands`
- `ua` = concatenated browser name + version

### CompanionRegClientUtils (Platform ID for Pairing)
**File:** `v4iuqx4/yD/l/en_GB-j/wvto_p76POX.js` (line 356)

```js
// Platform enum used during companion_hello:
// UNKNOWN=0, CHROME=1, EDGE=2, FIREFOX=3, IE=4, OPERA=5,
// SAFARI=6, ELECTRON=7, UWP=8, OTHER_WEB_CLIENT=9

function d(){
  if(isWindows) return UWP;
  var name = WAWebBrowserInfo().name;
  // Chrome→CHROME, Firefox→FIREFOX, IE→IE, Opera→OPERA,
  // Safari→SAFARI, Edge→EDGE
}
```

---

## 4. Browser & Device Fingerprinting (WAM)

### WAWebDeviceFeatures — Browser Signal Collection
**Module:** `WAWebDeviceFeatures`  
**File:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue-HJzVAh3RSGse3h5...` (line 322)

Collects all these browser signals:
```js
{
  platformEstimate: navigator.platform,          // "Win32", "MacIntel", "Linux x86_64"
  deviceMemory: navigator.deviceMemory,          // 8, 4, undefined
  screenResolution: devicePixelRatio * outerWidth + "x" + devicePixelRatio * outerHeight,
  connectionRtt: navigator.connection?.rtt ?? -1,  // Round-trip time
  sessionStorageLength: sessionStorage.length,
  touchPresence: "ontouchstart" in window && "TouchEvent" in window,
  gpuMake: WebGL UNMASKED_RENDERER_WEBGL,       // GPU vendor/renderer string
  webglFingerprint: WebGL shader-based canvas fingerprint (50x50 canvas),
  timezone: Intl.DateTimeFormat resolved timezone,
  canvasFingerprint: Canvas 2D fingerprint (rendered text + color),
  automationSignals: browser automation detection,
  chromeStructure: chrome object structure check,
  foreignDbList: indexedDB database enumeration
}
```

### WAWebWamFingerprint — Combined Fingerprint
**Module:** `WAWebWamFingerprint`  
**File:** `v4ix4p4/ye/l/en_GB-j/QYtmU9dW-Td.js` (line 1143)

```js
async function getFingerprint(){
  return {
    webdriver: navigator.webdriver,       // true if automated
    browserEngine: DETECTED_BROWSER_ENGINE, // "Blink", "WebKit", "Gecko"
    ...(await WAWebBrowserFeatures()),
    ...WAExternalInjectedSources(),
    ...WAWebDeviceFeatures()
  }
}
```

### WAWebWamFingerprintReporter — Sends Fingerprint via WAM
**Module:** `WAWebWamFingerprintReporter`  
**File:** `v4ix4p4/ye/l/en_GB-j/QYtmU9dW-Td.js` (line 1145)

Logs ALL fingerprint data as a `WebcFingerprintWamEvent` (WAM event):
```js
async function logFingerprintToWam(){
  var fp = await WAWebWamFingerprint();
  var event = new WebcFingerprintWamEvent({
    webcWindowNavigatorWebdriver: fp.webdriver,
    browserEngine: fp.browserEngine,
    hasChrome: fp.hasChrome,
    hasTaskbar: fp.hasTaskbar,
    hasWebShare: fp.hasWebShare,
    mimeTypeCount: fp.mimeTypeCount,
    notificationPermission: fp.hasNotificationPermission,
    pdfViewerEnabled: fp.isPDFViewerEnabled,
    pluginCount: fp.pluginCount,
    platformEstimate: fp.platformEstimate,
    extentionIds: fp.extensionIds,
    externalSources: fp.externalSources,
    historyLength: fp.historyLength,
    viewportSize: fp.viewportSize,
    cpuMake: fp.cpuPlatform,               // navigator.platform
    deviceMemory: fp.deviceMemory,
    screenResolution: fp.screenResolution,
    connectionRtt: fp.connectionRoundTripTime,
    touchPresence: fp.touchPresence,
    sessionStorageLength: fp.sessionStorageLength,
    webcCanvasFingerprint: fp.canvasFingerprint,
    gpuMake: fp.gpuMake,
    timezone: fp.timezone,
    webcWebglFingerprint: fp.webglFingerprint,
    automationSignals: fp.automationSignals,
    chromeStructure: fp.chromeStructure,
    foreignDbList: fp.foreignDbList
  });
  event.commit();
}
```

### WAWebFingerprintJob — Trigger
**Module:** `WAWebFingerprintJob`  
Dispatched as a low-priority non-persisted job at startup:
```js
function sendWamFingerprintJob(){
  return createNonPersistedJob("collectWebFingerprints",
    () => WAWebWamFingerprintReporter.logFingerprintToWam(),
    { priority: JOB_PRIORITY.LOW }
  ).waitUntilCompleted()
}
```

**Note:** Bot clients that don't run JavaScript cannot collect these signals but also don't need to — the mere ABSENCE of the `WebcFingerprintWamEvent` WAM event is a detectable signal.

---

## 5. WAM Telemetry System

**Module:** `WAWebWam`  
**File:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue-HJzVAh3RSGse3h5...` (line 993)  
**Codegen:** `WAWebWamCodegenUtils` — used in ALL WAM event definitions

### Architecture
- Events defined via `WAWebWamCodegenUtils.defineEvents({Name:[ID, fields, flags, privacy, privateId]}, {})`
- Events have **32-bit integer IDs** (not string names)
- Runtime: `WAWebWam` — provides `commit()`, `set()`, `resumeJobs()`, `initialize()`
- Periodic flush to server via `<iq xmlns="w:stats"><add t="ts">buffer</add></iq>`
- Background tab: **skips WAM processing entirely**
- Privacy tiers: `"regular"` (standard), `"private"` (with private numeric ID)

### Complete Event ID Table
Extracted to `wam_event_map.json` (422 events). Key events:

| Event Name | ID | Description |
|---|---|---|
| GroupCreateC | 156 | Group created |
| GroupJoinC | 158 | Joined group |
| MessageReceive | 450 | Message received |
| Login | 460 | Login event |
| E2eMessageSend | 476 | E2E message sent |
| E2eMessageRecv | 478 | E2E message received |
| CrashLog | 494 | Crash report |
| WebcPageLoad | 642 | Page loaded |
| MessageSend | 854 | Message sent |
| WebcChatOpen | 864 | Chat opened |
| AppLaunch | 1094 | App launched |
| MediaPicker | 1038 | Media picker used |
| ContactSyncEvent | 1006 | Contact sync |
| StatusPost | 1176 | Status posted |
| UiAction | 472 | UI interaction |
| ContactUsSession | 470 | Contact us session |
| WebcLogin | 1664 | Web login |
| WamClientErrors | 1144 | Client errors |
| WebcFingerprintWamEvent | — | Browser fingerprint |
| WebcPageLoad2WamEvent | — | Page load timing |
| DefenseModeClick | 7096 | Defense mode interaction |
| MessageDeleteActions | 3626 | Message deletion |
| BlockEventsFs | 4288 | Block/unblock events |
| ContactSearchExperience | 6574 | Contact search |
| ChatLockAction | 4212 | Chat lock |
| MdChatAssignment | 3752 | Chat assignment |
| GroupSuspensionAppealEvents | 7574 | Suspension appeals |
| StatusPogImpression | 8302 | Status impression |

### Background Tab Behavior
Critical finding: Background tabs skip WAM processing:
```js
// WAWebWam.js
var isBackground = !WAWebEnvironment.isGuest &&
  !WAWebUserPrefsTabMutex.currentTabHasMutex();
if(isBackground){
  // Skip WAM processing entirely
  return;
}
```

This means a bot that sends WAM events while "in background" (no browser visible) would actually match real behavior — but only if it skips processing sometimes.

---

## 6. Logout Reasons & Ban Codes

**Module:** `WAWebLogoutReasonConstants`  
**File:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue-HJzVAh3RSGse3h5...` (line 937)

### Complete Logout Reason Enum (37 reasons)
```js
var LogoutReason = {
  UserInitiated:                     "user_initiated",
  SyncdFailure:                      "syncd_failure",
  InvalidAdvStatus:                  "invalid_adv_status",
  CriticalSyncTimeout:               "critical_sync_timeout",
  SyncdTimeout:                      "syncd_timeout",
  HistorySyncTimeout:                "history_sync_timeout",
  AccountSyncTimeout:                "account_sync_timeout",
  MDOptOut:                          "md_opt_out",
  UnknownCompanion:                  "unknown_companion",
  ClientVersionOutdated:             "client_version_outdated",
  SyncdErrorDuringBootstrap:         "syncd_error_during_bootstrap",
  AccountSyncError:                  "account_sync_error",
  StorageQuotaExceeded:              "storage_quota_exceeded",
  PrimaryIdentityKeyChange:          "primary_identity_key_change",
  MissingEncSalt:                    "missing_enc_salt",
  MissingScreenLockSalt:             "missing_screen_lock_salt",
  AccountLocked:                     "account_locked",
  LidMigrationSplitThreadMismatch:   "lid_migration_split_thread_mismatch",
  LidMigrationNoLidAvailiable:       "lid_migration_no_lid_available",
  LidMigrationPrimaryMappingsObsolete:"lid_migration_primary_mappings_obsolete",
  LidMigrationPeerMappingsNotReceived:"lid_migration_peer_mapping_not_received",
  LidMigrationStateDiscrepancy:      "lid_migration_state_discrepancy",
  LidMigrationPeerMappingsMalformed: "lid_migration_peer_mapping_malformed",
  LidMigrationFailedToParseMapping:  "lid_migration_failed_to_parse_mapping",
  LidMigrationCompanionIncompatibleKillswitch: "lid_migration_companion_incompatible_killswitch",
  LidMigrationOneOnOneThreadMigrationInternalError: "lid_migration_one_on_one_thread_migration_internal_error",
  LidMigrationUnmigratedPrimary:     "lid_migration_unmigrated_primary",
  LidMigrationUnmigratedCompanion:   "lid_migration_unmigrated_companion",
  LidBlocklistPnWhenMigrated:        "lid_blocklist_pn_when_migrated",
  LidBlocklistChatDbUnmigrated:      "lid_blocklist_chat_db_unmigrated",
  WebFailAddChat:                    "web_fail_add_chat",
  WebFailOfflineResume:              "web_fail_offline_resume",
  WebFailStorageInitialization:      "web_fail_storage_initialization",
  WebFailEncSalt:                    "web_fail_enc_salt",
  CacheStorageOpenFailed:            "cache_storage_open_failed"
};
```

### Error Code Mapping
```js
var errorCodes = {
  CLIENT_FATAL:    "0",
  SYNC_FAIL:       "1",
  INITIATIVE:      "2",
  PROTOCOL_ERROR:  "3",
  // ... more codes
};
```

### Logout Reason → Error Code
Function `getErrorCodeFromLogoutReason` maps LogoutReason to server-facing error codes.

### Bridge Ban Strings (from WAWebSocketLogoutJob)
Found in `WAWebSocketLogoutJob`:
- `"service_unavailable_503_from_bridge"`
- `"account_temporarily_banned_from_bridge"`
- `"starting_logout_from_bridge"`
- `"unexpected_logout_modal_from_bridge"`

These are bridge-specific ban/hard-logout signals received via WebSocket, suggesting the server can force-logout a companion device.

---

## 7. Anti-Spam / Reach-Out Timelock

**Module:** `WAWebReachoutTimelockUtils`  
**Files:** Multiple bundles (e.g., `v4iyPC4/yW/l/en_GB-j/n4jmwYkjRtg.js` line 266)

### What It Is
A server-side enforcement mechanism that **restricts outbound messaging** when WhatsApp detects unusual reach-out patterns (sending to too many contacts who haven't messaged you first).

### Key Functions
```js
isUserReachoutTimelocked()            // Is the user currently timelocked?
isUserReachoutTimelockedAndVisible()  // Is timelock active AND visible to UI?
canSendMsgWhileTimelocked({contact, chat}) // Can send to this specific contact?
```

### UI Restrictions
- `WAWebReachoutTimelockRestrictedModalLoadable` — modal shown when trying to message while timelocked
- `WAWebReachoutTimelockButterBarLoadable` — banner notification
- `WAWebReachoutTimelockBridgeApi` — bridge API for native clients
  - `reachoutTimelockEnforcementLifted({enforcementType, shouldPresentRestrictionsLiftedModal})`
  - `reachoutTimelockEnforcementSet()`
- When enforcement lifts: sets `canSendMsgWhileTimelocked = true` on all contacts

### WAM Event
DefenseModeClick event (ID 7096) logs defense mode interactions including timelock state.

---

## 8. Defense Mode / Brigading Protection

**Module:** `WAWebDefenseModeUtils`  
**Also:** `WAWebBrigadingButterBar`  
**Files:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue...` (line 671), `v4ixt44/yd/l/en_GB-j/OQ1XhNYCI_C.js` (line 84)

### Defense Mode (Strict Account Settings)
Three states: `DETECTED`, `ENFORCED`, `NOT_ENFORCED`

When active, blocks messages from unknown accounts. Protects these privacy settings:
- Profile photo, About, Groups, Last Seen, Online
- Link previews, Security notifications
- Block unknown account messages
- Protect IP in calls, Silence unknown callers

### Brigading Butter Bar
Shows warnings:
- `DETECTED`: "Your device is receiving a high volume of messages. Block messages from unknown accounts."
- `ENFORCED`: "Messages from unknown accounts are being blocked."
- `NOT_ENFORCED`: "Message blocking turned off."

### Quarantine System
- `WAWebQuarantineDataStore` — stores quarantined attachment metadata
- `WAWebBulkUnquarantineMessagesJob` — bulk unquarantine when defense mode is turned off
- `WAWebDefenseModeQuarantineLogger` — logs quarantine events

---

## 9. MQTT Connection Logging (Falco)

**Module:** `MqttLogger`  
**Files:** `v4izBr4/yL/l/en_GB-j/Rtw3CSaSwEj...` (line 1520) and all other bundles

### Events Tracked Per Connection
```
eventLogConnect: {
  ackReceived, attemptNumber, connectionState, connectionStatus,
  disconnectCount, duration, errorMessage, failTotal, hostname,
  messageSizeBytes, osConnectivity, sessionID, successTotal, total_duration
}
```

### Falco Event IDs
- `MqttUnifiedClientConnectFalcoEvent` — Falco ID `1744057` (event name: `"mqtt_unified_client_connect"`)
- `MqttUnifiedClientDisconnectFalcoEvent` — Falco ID `1744058`

### Key Metrics Logged
```js
connect_success_rate = successTotal / (successTotal + failTotal)
device_id = ChannelClientID
hostname = WebSocket server hostname
connectionStatus = "success" | "failed"
session_id = unique session identifier
ws_count = global WebSocket stream counter
```

### Error Event Types
```js
mqtt_client_error, mqtt_client_connect, mqtt_client_disconnect,
mqtt_client_publish, mqtt_qos1_publish_timeout, mqtt_protocol_error
```

---

## 10. Reconnect Backoff & Stability

**Module:** `WAWebCommsConfig` (referenced in multiple places, exact location in v4ivPn4 bundle)  
**Also:** `WAWebConnectAttemptManager` (connected to ClientPayload)

### Connect Attempt Count
Persisted in ClientPayload module:
```js
var connectAttemptCount = 0;  // module-level counter
function increment(){ connectAttemptCount++; }
```
- Incremented on each connect attempt
- Reset to 0 after successful heartbeat post-connect
- Included in ClientPayload as `connectAttemptCount`
- **This is a detection signal** — if a bot reconnects rapidly without proper backoff, the count grows

### Reconnect Delays (from CommsConfig)
- Base delay: 500ms
- Max delay: 10s
- 503 backoff: 300s (5 minutes)
- Jitter formula applied to all delays
- Stability tracking: counters track connection success/failure ratios

### WS Reconnect Behavior
Real WhatsApp Web:
1. On disconnect: wait base delay (500ms) with jitter
2. On 503: wait 5 minutes
3. On repeated failure: exponential backoff up to 10s
4. Increment `connectAttemptCount`
5. On successful HB after connect: reset counter to 0

---

## 11. Syncd Device Fingerprint

**Module:** `WAWebSyncdKeyCallbacksApi`  
**File:** `v4ix4p4/ye/l/en_GB-j/QYtmU9dW-Td.js` (line 968)

### getDeviceFingerprint
```js
async function getDeviceFingerprint(){
  var list = await getMyDeviceList();
  return {
    currentIndex: list.currentIndex,
    deviceIndexes: list.devices.map(d => d.keyIndex),
    rawId: list.rawId
  }
}
```

This is NOT the browser fingerprint — it's the AppState sync device list fingerprint (key indexes of all linked devices). Used for:
- Key rotation (when device removed or key expires)
- Syncd key management
- Missing key requests

### Key Rotation Triggers
```js
hasKeyExpired(key)                    // Check if sync key expired
hasADeviceBeenRemoved(key, fp)        // Check if device removed from fingerprint
rotateKey(fp, oldKey)                 // Generate new key
```

---

## 12. Presence & Typing Behavior

**Module:** `WAWebPresenceChatAction`  
**File:** `v4iy2x4/ya/l/en_GB-j/m4428zUhFBz.js`

### Timings
- **Typing → 2.5s idle → send `paused`**
- **Typing timeout: 10s** — after 10s continuous typing, treat as idle/paused
- `setPresenceAvailable()` — called on connect and periodically
- `setPresenceUnavailable()` — called when tab loses focus / browser closes

### Stanza Format
```
<presence type="available" />          // Online
<presence type="unavailable" />        // Offline
<chatstate xmlns="urn:xmpp:chatstates">
  <composing />                        // Typing
  <paused />                           // Stopped typing
</chatstate>
```

---

## 13. Read Receipts

**Module:** `WAWebSendPlayedReceiptJob`  
**File:** `v4izPN4/y9/l/en_GB-j/cItmOMC1aCZBPAIShs87iGHUNQvs0LwBljsMaa9vTcvzxijNAr5MmxoAnMGGjOUOU8zXeL6fuKR8I5tURb0HaVnEbv028NSUk97Xg86g7HNh9rGVJpMrazftrZBooypub7TqzKLP-kaD5T1BSD74T4JV_n.js`

### Behavior
- Sends `receipt` stanza with `type=played` or `type=played_self`
- `PLAYED_SELF` (server-side only) is used when read receipt privacy is set to "none"
- Normal read receipts use `PLAYED`
- Sent as a job (`WAWebSendPlayedReceiptJob`)

### Privacy Setting
```js
if(readReceipts === "none"){
  sendReceipt("played_self")    // server-side only
} else {
  sendReceipt("played")         // visible to sender
}
```

---

## 14. Connect Attempt Count

**Module:** `WAWebClientPayload` (internal counter `u`)  
**Persisted in:** `WAWebConnectAttemptManager` via `SiteData`

### Behavior
- `connectAttemptCount` starts at 0
- Incremented each time the client initiates a WebSocket connection
- Sent in `ClientPayload` as a field
- Reset to 0 only after a **successful heartbeat** following connection
- A bot that reconnects frequently will have a growing `connectAttemptCount`

### Reset Logic
The reset happens on successful HB, not on connect. This means:
- Successful connect + HB = counter reset to 0
- Successful connect + NO HB (disconnect before HB) = counter NOT reset
- Multiple failed connections = counter grows unbounded

---

## 15. Page Load Logging

**Module:** `WAWebPageLoadLoggingImpl`  
**File:** `v4ivPn4/yc/l/en_GB-j/Q28d6Be5FfQBmeNsUUcxwMRSA7ue-HJzVAh3RSGse3h5...` (line 1384)

### Signals Collected on Page Load
```js
{
  qr_screen: boolean,           // Was QR screen shown?
  qr_screen_experience: number, // QR screen experiment variant
  is_foreground: boolean,       // Not document.hidden
  page_load_id: uuid,           // Random UUID per page load
  sub_platform: "web" | "win_hybrid",
  referrer: document.referrer,  // Originating site
  random_user_id: cyrb53(wa_ul_cookie),  // Hashed cookie
  locale: string,               // User locale
  touch_presence: boolean,      // DeviceFeatures.touchPresence
  encrypted_rid: string,        // Encrypted RID if registered
  socket_error_count: number    // Running count of socket errors
}
```

### Timeout
Page load marked as failed if not complete within **120s** (`y = 12e4`).

### WAM Event
`WebcPageLoad2WamEvent` is committed on page load completion with `webcPageLoadId`.

---

## 16. Key Rotation & Device List Sync

**Modules:** `WAWebSyncdKeyManagement`, `WAWebSyncdRotateKey`, `WAWebSyncDeviceAdvDeviceListJob`  
**File:** `v4ix4p4/ye/l/en_GB-j/QYtmU9dW-Td.js` (line 1052)

### Key Rotation Triggers
1. **Key expiry** — `hasKeyExpired(key)` checks if key is too old
2. **Device removal** — `hasADeviceBeenRemoved(key, fingerprint)` checks device list changes

### Device List Sync
```js
syncDeviceAdvDeviceListJob(wids, null)
```
- Called during account sync
- Synchronizes linked device list
- Passes `device_hash`, `ts`, `expected_ts`, `phashV2` in the request
- Used to detect new/removed devices

### USync Retry
- Max 3 retries with random 10s jitter
- Used for device discovery

---

## 17. Implications for Bot Clients

### Critical Detection Signals (Server-Side Observable)

| Signal | WA Web Behavior | Bot Implication |
|---|---|---|
| **ClientPayload.connectAttemptCount** | Resets to 0 after successful HB | Must NOT grow unbounded on reconnects |
| **ClientPayload.UserAgent.phone_id** | NOT set | Must NOT be set (current behavior is correct) |
| **buildHash** | MD5 of VERSION_BASE | Must use MD5, not SHA-256 |
| **DeviceProps.Version** | OS version parsed from real UA | Must be from real OS, not hardcoded |
| **DeviceProps.requireFullSync** | Dynamic based on gkx flags | Should NOT be hardcoded `true` |
| **WAM events** | Sent periodically | Absence is detectable |
| **Browser fingerprint WAM event** | Sent at startup | Absence is detectable |
| **Presence available** | Set on connect | Absence is detectable |
| **Read receipts** | Sent with delay | Absence or instant is detectable |
| **Typing indicators** | 2.5s idle → paused, 10s timeout | Wrong timing is detectable |
| **Reconnect backoff** | 500ms base, 10s max, 300s on 503 | Wrong timing is detectable |
| **MQTT connect/disconnect events** | Logged to Falco | Cannot fake (server-side) |
| **Page load signal** | Sent on web.open | Absence is detectable |
| **Key rotation** | Triggered on key expiry/device removal | Must participate in AppState sync |

### Anti-Spam Systems (Server-Enforced)

1. **Reach-Out Timelock** — Server restricts outbound messaging when unusual reach-out detected. Client gets a `reachoutTimelockEnforcementSet` signal. Bot should stop sending when this is received.

2. **Defense Mode** — Blocks messages from unknown accounts. Has `DETECTED`, `ENFORCED`, `NOT_ENFORCED` states. Related to brigading protection.

3. **Brigading Detection** — "Your device is receiving a high volume of messages" — rate limiting on incoming messages.

### Server-Forced Logout Reasons
The server can force-logout a companion device for:
- `AccountLocked` — account banned
- `ClientVersionOutdated` — version too old
- `StorageQuotaExceeded` — media storage limit
- `PrimaryIdentityKeyChange` — security event on primary
- `WebFailStorageInitialization` — IndexedDB/storage errors
- `UnknownCompanion` — device not recognized
- Various LID migration errors

### WAM Events Most Likely to Matter
Based on frequency and criticality, the minimum WAM events a bot should send:
- `Login` (460) — on login
- `AppLaunch` (1094) — on startup
- `MessageSend` (854) — on message send
- `MessageReceive` (450) — on message receive
- `WebcFingerprintWamEvent` — browser fingerprint (hard to fake without real browser)
