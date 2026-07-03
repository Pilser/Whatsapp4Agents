# Client Fingerprinting Comparison

How each library presents itself to WhatsApp's server during the Noise handshake (`ClientPayload.UserAgent`) and pairing registration (`DeviceProps`). These fields are the primary signals WhatsApp uses for client identification and abuse detection.

---

## 1. Noise Handshake: `ClientPayload.UserAgent`

| Field | whatsmeow (Go) | Baileys (JS) | whatsapp-rust (Rust) | zeroclaw |
|---|---|---|---|---|
| **Platform** | `WEB` | part of browser tuple | `Web` | same as rust |
| **Device** | `"Desktop"` | browser string (e.g. `"Chrome"`) | `"Desktop"` | same |
| **Manufacturer** | `""` | — | `""` | same |
| **OsVersion** | `"0.1"` | varies by preset e.g. `"14.4.1"` | `"0.1.0"` | same |
| **OsBuildNumber** | `"0.1"` | — | = os_version | same |
| **ReleaseChannel** | `RELEASE` | — | `RELEASE` | same |
| **WebSubPlatform** | `WEB_BROWSER` | implicit | `WebBrowser` | same |
| **phone_id** | **not set** | **not set** | **not set** (was random UUID, fixed) | same |
| **Locale** | `en-US` | `US` (countryCode) | `en-US` | same |
| **Mcc/Mnc** | `"000"` | — | `"000"` | same |
| **ConnectType** | `WIFI_UNKNOWN` | — | `WIFI_UNKNOWN` | same |
| **ConnectReason** | `USER_ACTIVATED` | — | `USER_ACTIVATED` | same |

## 2. Pairing: `DeviceProps` (companion_registration)

| Field | whatsmeow (Go) | Baileys (JS) | whatsapp-rust (Rust) | zeroclaw |
|---|---|---|---|---|
| **OS** | `"whatsmeow"` | not set directly | `"rust"` | **`"ZeroClaw"`** |
| **PlatformType** | `UNKNOWN` → `OtherWebClient` (`'9'`) | maps browser UP → default `Chrome` (`'1'`) | `Unknown` → `OtherWebClient` (`'9'`) | **`Desktop` → `Electron` (`'7'`)** |
| **Version** | `0.1.0` | — | `0.1.0` | default |
| **RequireFullSync** | `false` | — | `false` (was `true`, fixed) | default |
| **HistorySyncConfig** | comprehensive | — | comprehensive | default |

## 3. Wire behavior anomalies

| Signal | whatsmeow | Baileys | whatsapp-rust | zeroclaw |
|---|---|---|---|---|
| `phone_id` set | ❌ | ❌ | ❌ (was random UUID, fixed to `None`) | ✅ (random UUID) |
| `DeviceProps.OS` is library name | ✅ (`"whatsmeow"`) | ❌ (not set) | ✅ (`"rust"`) | ✅ (`"ZeroClaw"`) |
| Version is realistic | ❌ (`0.1.0`) | ✅ (fetches latest real version) | ❌ (`0.1.0`) | ❌ (`0.1.0`) |
| Uses `require_full_sync=true` | ❌ | ❌ | ❌ (was `true`, fixed to `false`) | ✅ |
| `PlatformType` matches display name | ❌ (`Unknown`→`"9"` but display says `"Chrome"`) | ✅ (`Chrome`→`"1"` matches display) | ❌ (`Unknown`→`"9"` but default display says `"Chrome (Linux)"`) | ✅ (`Desktop`→`'7'`→`Electron`→`"Chrome (ZeroClaw)"`) |
| **buildHash algorithm** | SHA-256? | — | MD5 (WA Web behavior) | same |
| **DeviceProps.version** parsed from OS version | ❌ (hardcoded) | ❌ (not set) | ❌ (hardcoded) | same |
| **WAM events** | Not sent | Not sent (infra only) | Not sent | Not sent |

## 3b. Reverse-Engineered WA Web Behavior

From extracted WhatsApp Web JS bundles (June 2026 session):

### WAWebBrowserInfo
```js
// Returns: {os: "Mac OS", version: "10.15.7", name: "Chrome", ua: "Chrome 135"}
// Uses WAWebUA parser + Edge detection via navigator.userAgentData.brands
```

### WAWebClientPayload - 3 entry points
- **getClientPayloadForLogin()** — sends `username`, `device`, `lc` (login counter), `connectAttemptCount` (persisted), `lidDbMigrated` flag
- **getClientPayloadForRegistration()** — builds on base payload adding `devicePairingData`:
  - `buildHash`: **MD5** of `VERSION_BASE` (NOT SHA-256)
  - `deviceProps`: encoded DeviceProps protobuf
  - `eRegid`, `eKeytype`, `eIdent`, `eSkeyId`, `eSkeyVal`, `eSkeySig`
- **getClientPayloadForRegularLogin()** — third path

### DeviceProps construction
- `OS`: from browser UA parser `e.os.name` (e.g. `"Mac OS"`, `"Linux"`, `"Windows"`)
- `Version`: OS version parsed into `primary/secondary/tertiary` integers
- `PlatformType`: mapped from browser name → enum:
  - Chrome→1, Firefox→3, IE→4, Opera→5, Safari→6, Edge→2, Electron→7, UWP→8
  - Windows always maps to UWP (8)
- `requireFullSync`: dynamic based on feature flags
- `historySyncConfig`: includes feature toggles for bot/inline/biz messages, CAG reactions, call logs, group history, thumbnail sync limit

### WAM Telemetry
- Uses `WAWebWamCodegenUtils.defineEvents()` with **32-bit integer IDs** (not strings)
- 422 unique event types extracted from JS bundles
- Events sent via `w:stats` IQ namespace
- Main module: `WAWebWam` with `commit()`, `set()`, `resumeJobs()`, `initialize()`

### UserAgent
- Fetches device info via `getDeviceInfo` backend call
- Sets `appVersion`, `platform: WEB`, `releaseChannel`, `mcc/mnc`, `osVersion`, `manufacturer`, `device`, `osBuildNumber`, `localeLanguageIso6391`, `localeCountryIso31661Alpha2`
- **phone_id NOT set** — this field exists in proto but WA Web never populates it

---

## 4. How WA Web version is determined

### Baileys (JS)

Has two fetch strategies, run at startup via `fetchLatestBaileysVersion` / `fetchLatestWaWebVersion`:

1. **fetchLatestBaileysVersion** — GET `https://raw.githubusercontent.com/WhiskeySockets/Baileys/master/src/Defaults/index.ts`, parses line 7 as `const version = [p, s, t]`. Falls back to hardcoded `[2, 3000, 1035194821]` on failure.
2. **fetchLatestWaWebVersion** — GET `https://web.whatsapp.com/sw.js` with browser headers, parses `client_revision`, builds `[2, 3000, revision]`. Fails if WhatsApp blocks the request.

The `update-version.ts` CI script (`.github/workflows/update-version.yml`) runs weekly, calling `fetchLatestWaWebVersion`, and updates hardcoded constants via PR if a newer version is found.

### whatsmeow (Go)

Hardcoded in `store/clientpayload.go`: `waVersion = {2, 3000, 1041871181}`. Can be overridden at runtime via `SetWAVersion()`.

### WAOS (Rust, overrides whatsapp-rust)

Implements 3-tier fallback at daemon startup:

```
GET https://web.whatsapp.com/sw.js
  with headers:
    User-Agent:      Mozilla/5.0 (X11; Linux x86_64; rv:100.0) Gecko/20100101 Firefox/100.0
    Sec-Fetch-Dest:  script
    Sec-Fetch-Mode:  no-cors
    Sec-Fetch-Site:  same-origin
    Referer:         https://web.whatsapp.com/
    Accept:          */*

Parses: client_revision\":<N>  →  [2, 3000, N]

Priority:
  CDN fetch succeeds → write to DB, use fetched version
  CDN fails           → read from DB cache → use cached version
  DB empty            → use configured version from config.toml / .env
```

Each path logs the source:
```
WhatsApp version: 2.3000.1042258947 (fetched from CDN)
WhatsApp version: 2.3000.1042249440 (from DB cache, fetch failed: HTTP 403)
WhatsApp version: 2.3000.1042249440 (from config, fetch failed: timeout, no cached version)
```

The `wa_version` table in the main DB stores the singleton row:
```sql
CREATE TABLE IF NOT EXISTS wa_version (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    primary_v INTEGER NOT NULL,
    secondary_v INTEGER NOT NULL,
    tertiary_v INTEGER NOT NULL,
    fetched_at INTEGER NOT NULL
);
```

---

## 5. WAOS-specific overrides

WAOS sits on top of `whatsapp-rust` and applies runtime overrides via `DevicePropsOverride`:

| Setting | whatsapp-rust default | WAOS override | Config source |
|---|---|---|---|
| **OS** | `"rust"` | `"macOS"` | `[device] os` in config.toml |
| **PlatformType** | `Unknown` → `'9'` | `Chrome` → `'1'` | `[device] platform_type` in config.toml |
| **WA Version** | `0.1.0` | Fetched from CDN / DB / config | 3-tier resolution at startup |
| **require_full_sync** | `true` | `false` | Hardcoded fix in `wacore/src/store/device.rs` |
| **phone_id** | Random UUID per connect | `None` (omitted) | Default in all `ClientProfile` constructors |

---

## Detectability Risk Assessment

### Low Risk
- **Baileys (JS)**: Fetches real WA Web version at runtime, browser strings match real browsers (`Mac OS / Chrome / 14.4.1`), `PlatformType` = `Chrome` which is the most common WA Web client. No library-named strings. Closest to mimicking a real browser.
- **whatsmeow (Go)**: `PlatformType` = `UNKNOWN` but wire byte is `'0'` which the server **rejects** during pairing — the Go code routes around this: `pair.go` switches on `PlatformType` and sends the correct wire byte via a different path. At runtime `BaseClientPayload` declares `Platform: WEB` + `Device: "Desktop"` which is clean. Minor risk: `OS: "whatsmeow"` in DeviceProps is library-named.
- **WAOS**: Overrides whatsapp-rust's defaults with `os: "macOS"`, `PlatformType: Chrome`, real WA Web version fetched from CDN, `require_full_sync: false`, `phone_id` omitted. Matches real WA Web fingerprint closely. The `os` field in DeviceProps is realistic (`"macOS"` instead of `"rust"`). Dynamic version keeps it current.

### Medium Risk
- **whatsapp-rust (Rust) raw**: `PlatformType` = `Unknown` → `'0'` which WA server rejects (same as whatsmeow). But the Rust library **does** emit it as-is (no routing workaround), so pairing succeeds only because `OtherWebClient` (`'9'`) is the actual default fallback in `companion_web_client_type_for_props`. `phone_id` was random UUID per connect (now fixed to `None` in all `ClientProfile` constructors). Had `require_full_sync: true` (now fixed to `false`). Realistic version not fetched at library level.

### Highest Risk
- **zeroclaw**: Inherits all risks from whatsapp-rust **plus** uses `PlatformType::Desktop` (`'7'` = `Electron`) with `OS: "ZeroClaw"` — these are unique identifiers with no real-world counterpart at scale. `PlatformType::Desktop` routes to `Electron` wire byte, which is used by WhatsApp Desktop (Electron) app, but combined with `"ZeroClaw"` OS and the `phone_id` UUID it creates a very distinctive fingerprint.

---

## What Real WhatsApp Web Looks Like

Real WA Web in Chrome:
- `UserAgent.Platform`: `WEB`, `Device: ""`, `Manufacturer: ""`
- No `phone_id` field at all
- `DeviceProps.OS`: `"Linux"` (or `"Mac OS"`, `"Windows"`)
- `DeviceProps.PlatformType`: matches the actual browser (`Chrome`/`Firefox`/`Edge`/`Safari`)
- `DeviceProps.Version`: real WA Web version number
- `WebInfo.WebSubPlatform`: `WEB_BROWSER`
- `ConnectType`: `WIFI_UNKNOWN`
- `ConnectReason`: `USER_ACTIVATED`

Real WA Desktop (Electron):
- `UserAgent.Platform`: `WIN` / `MACOS` (varies by OS)
- `DeviceProps.PlatformType`: `Desktop` → `'7'` (Electron)
- No `WebInfo` section (desktop client omits it)
- `DeviceProps.OS`: `"Windows"` / `"macOS"` / `"Linux"`

---

## Recommendations to Reduce Detectability

| # | Recommendation | Status |
|---|---|---|
| 1 | Remove `phone_id` | ✅ Done — `None` by default (matches WA Web, confirmed by RE) |
| 2 | Use realistic OS name | ✅ Done — WAOS config sets `"macOS"`, overrides library default `"rust"` |
| 3 | Use `PlatformType::Chrome` | ✅ Done — WAOS config sets `Chrome` → wire byte `'1'` |
| 4 | Fetch real WA Web version | ✅ Done — 3-tier: CDN sw.js → DB cache → config fallback |
| 5 | Set `require_full_sync: false` | ✅ Done — `wacore/src/store/device.rs:208` changed to `false` |
| 6 | Use `buildHash = MD5(VERSION_BASE)` | 🟡 Not verified — WA Web uses MD5, not SHA-256 |
| 7 | WAM event ID mapping | ✅ Available — 422 event types at `/tmp/wam_event_map.json` |
