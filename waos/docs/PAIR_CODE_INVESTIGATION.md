# Pair Code Authentication — Investigation Notes

## Problem

`pair_with_code()` returns `400 bad-request` from WhatsApp server for phone number `256755304241`. The IQ is structurally valid but WhatsApp rejects it.

## What We Compared

We compared our Rust implementation against a working Baileys (JavaScript) implementation at `/home/pilser/AA-Projects/WhatsappWebBot`.

## Pairing IQ Structure — Side by Side

### Baileys JS (works)
```xml
<iq to="s.whatsapp.net" type="set" id="..." xmlns="md">
  <link_code_companion_reg
      jid="256755304241@s.whatsapp.net"
      stage="companion_hello"
      should_show_push_notification="true">
    <link_code_pairing_wrapped_companion_ephemeral_pub>[80 bytes]</>
    <companion_server_auth_key_pub>[32 bytes]</>
    <companion_platform_id>1</companion_platform_id>
    <companion_platform_display>Chrome (Baileys)</companion_platform_display>
    <link_code_pairing_nonce>0</link_code_pairing_nonce>
  </link_code_companion_reg>
</iq>
```

### Rust (fails with 400)
```xml
<iq xmlns="md" type="set" to="s.whatsapp.net" id="...">
  <link_code_companion_reg
      jid="256755304241@s.whatsapp.net"
      stage="companion_hello"
      should_show_push_notification="true">
    <link_code_pairing_wrapped_companion_ephemeral_pub>[80 bytes]</>
    <companion_server_auth_key_pub>[32 bytes]</>
    <companion_platform_id>7</companion_platform_id>
    <companion_platform_display>Electron (WAOS)</companion_platform_display>
    <link_code_pairing_nonce>0</link_code_pairing_nonce>
  </link_code_companion_reg>
</iq>
```

### Verified Matching

| Aspect | Status |
|--------|--------|
| Phone number format (digits only, no `+`, no leading `0`) | ✅ Match |
| JID format (`phone@s.whatsapp.net`) | ✅ Match |
| `link_code_pairing_wrapped_companion_ephemeral_pub` (80 bytes: salt+iv+encrypted) | ✅ Match |
| `companion_server_auth_key_pub` (noise key public, 32 bytes) | ✅ Match |
| `link_code_pairing_nonce` = `"0"` | ✅ Match |
| IQ namespace `xmlns="md"`, type `set` | ✅ Match |
| Target server `s.whatsapp.net` | ✅ Match |

### Differences Found

| Field | Baileys | Rust | Impact |
|-------|---------|------|--------|
| `companion_platform_id` | `1` | `7` | Possibly rejected by WhatsApp |
| `companion_platform_display` | `Chrome (Baileys)` | `Electron (WAOS)` | Display-only |
| IQ attr order | `to` then `type` then `id` | `xmlns` then `type` then `to` then `id` | Should not matter |

### Platform ID Mapping

`companion_platform_id` is a single ASCII character:

| Char | CompanionWebClientType | Source |
|------|----------------------|--------|
| `1` | Chrome | Default, Baileys |
| `7` | Electron | Our `PlatformType::Desktop` override |

WhatsApp might reject `Electron` (7) for third-party clients, expecting `Chrome` (1).

## How Baileys Configures the Socket

```typescript
browser: ['Baileys', 'Chrome', '1.0.0'],
```

- `browser[0]` → device `os` field = `'Baileys'`
- `browser[1]` → `platformType` → `PlatformType.CHROME` → companion_platform_id = `'1'`
- `browser[2]` → version string (not sent in pairing IQ)

In the `generateRegistrationNode` (which runs after successful pair), Baileys also sends:
```typescript
version: { primary: 10, secondary: 15, tertiary: 7 }
```
This is NOT sent during the initial pairing IQ — only after the phone confirms the code.

## Our Rust Device Props Override

```rust
DevicePropsOverride::new()
    .with_os("WAOS")
    .with_platform_type(PlatformType::Desktop),
```

This produces:
- `companion_platform_id` = `'7'` (CompanionWebClientType::Electron)
- `companion_platform_display` = `"Electron (WAOS)"`

## Unverified Hypotheses

1. **Platform ID `'7'` (Electron) is rejected** by WhatsApp for pair code auth — only `'1'` (Chrome) or standard browser types are accepted
2. **Rate limiting on the phone number** — repeated attempts with same number trigger a cooldown
3. **Phone number not registered on WhatsApp** — the server returns 400 if the number has no WhatsApp account

## Next Steps for Investigation

- Test with `PlatformType::Chrome` instead of `Desktop` to match Baileys
- Verify the phone number is registered on WhatsApp via the `IsOnWhatsApp` query
- Try a fresh phone number that hasn't been rate-limited
- Wait 10+ minutes between attempts to let any cooldown expire
