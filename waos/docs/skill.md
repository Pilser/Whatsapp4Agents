# WAOS — Headless WhatsApp for AI agents

## CLI Reference

### Account & Status

| Command | What it does |
|---|---|
| `waos status` | List accounts with connection status |
| `waos pair <alias>` | Instructions to pair a new phone |
| `waos daemon` | Start the daemon (bots, API, IPC) |

### Dashboard

| Command | What it does |
|---|---|
| `waos dashboard show` | Show live dashboard JSON (accounts, unread, health) |
| `waos dashboard watch` | Stream dashboard updates live |

### Chats

| Command | What it does |
|---|---|
| `waos chat --account <name> list` | List all chats, most recent first |
| `waos chat --account <name> unread` | List only chats with unread messages |
| `waos chat --account <name> search <query>` | Search messages across all chats |
| `waos chat --account <name> history <chat_id>` | Show last 5 messages with `is_me` flag. Use `--limit N`, `--before TS` (older), `--after TS` (newer), `--offset N` (skip), `--total` (show total count) |
| `waos chat --account <name> mark-read <chat_id>` | Mark a chat as read |
| `waos chat --account <name> listen <chat_id>` | Stream real-time events (queue updates, receipts, messages, typing). Use `--timeout N` (default 120s). Auto-closes after timeout — agent can reconnect |

### Messages

| Command | What it does |
|---|---|
| `waos message --account <name> send <chat_id> <text>` [--wait <secs>] | Queue a text message. With `--wait <secs>`, blocks for send confirmation |
| `waos message --account <name> reply <chat_id> <reply_to_id> <text>` [--wait <secs>] | Queue a reply. With `--wait <secs>`, blocks for send confirmation |
| `waos message --account <name> react <chat_id> <message_id> <emoji>` [--wait <secs>] | Queue a reaction. With `--wait <secs>`, blocks for send confirmation |
| `waos message --account <name> edit <chat_id> <message_id> <text>` [--wait <secs>] | Queue an edit. With `--wait <secs>`, blocks for send confirmation |
| `waos message --account <name> delete <chat_id> <message_id>` [--wait <secs>] | Queue a delete. With `--wait <secs>`, blocks for send confirmation |

### Media

| Command | What it does |
|---|---|
| `waos media --account <name> send <chat_id> <file_path>` [--wait <secs>] | Send media with optional `--text` caption, `--reply-to <msg_id>`, and `--as-document` (force document type). Auto-detects image/video/audio from extension. With `--wait <secs>`, blocks for send confirmation |
| `waos media --account <name> download <message_id>` | Look up media metadata |

### Contacts

| Command | What it does |
|---|---|
| `waos contact --account <name> search <query>` | Search contacts by name or phone |
| `waos contact --account <name> info <jid>` | Full profile: scores, memory, graph |
| `waos contact --account <name> score <jid> <0.0-1.0>` | Set importance score |

### Memory

| Command | What it does |
|---|---|
| `waos memory --account <name> get <chat_id>` | Get conversation memory for a chat |
| `waos memory --account <name> set <chat_id> <json>` | Store conversation memory |
| `waos memory --account <name> search <query>` | Search memory (vector or LIKE) |

### Graph

| Command | What it does |
|---|---|
| `waos graph --account <name> shared-groups <jid1> <jid2>` | Find shared groups |
| `waos graph --account <name> introductions <jid>` | Find intro paths |
| `waos graph --account <name> important-contacts <community_id>` | Key contacts in community |

### Communities

| Command | What it does |
|---|---|
| `waos community --account <name> list` | List communities |
| `waos community --account <name> info <community_id>` | Community details + members |

### Status (Stories)

| Command | What it does |
|---|---|
| `waos status list` | List accounts with connection status |
| `waos status send-text --account <name> <text>` [--bg-color <hex>] [--font <n>] [--wait <secs>] | Post a text status/story visible to all contacts. Optional `--bg-color` (0xAARRGGBB, default `0xFF1E6E4F`) and `--font` (0-4, default 0). With `--wait <secs>`, blocks for confirmation |
| `waos status send-image --account <name> <file_path> --thumbnail-file <thumb.jpg>` [--caption <text>] [--wait <secs>] | Post an image status/story. `--thumbnail-file` is required — use external tools (Python/Pillow, ffmpeg) to generate a JPEG thumbnail. With `--wait <secs>`, blocks for confirmation |
| `waos status send-video --account <name> <file_path> --thumbnail-file <thumb.jpg>` [--caption <text>] [--wait <secs>] | Post a video status/story. `--thumbnail-file` is required. With `--wait <secs>`, blocks for confirmation |
| `waos status revoke --account <name> <message_id>` [--wait <secs>] | Delete a previously posted status. With `--wait <secs>`, blocks for confirmation |
| `waos status history --account <name>` [--status <filter>] [--before TS] [--after TS] [--limit N] [--offset N] | List posted status history with date range + pagination. Default limit is 5. `--status expired` filters to statuses older than 24h. Each entry has `is_expired`, `media_path`, `thumbnail_path` etc. so agent can re-upload with new caption |
| `waos status delete --account <name>` [--status <filter>] [--older-than TS] [--newer-than TS] [--yes] | Delete status_history entries by date range. **Dry-run by default** — add `--yes` to actually delete. DB cleanup only, does NOT revoke from WhatsApp |

### Tasks

| Command | What it does |
|---|---|
| `waos task list [--status <status>]` | List tasks (opt filter) |
| `waos task complete <task_id>` | Mark task completed |

## Agent Workflow

```
waos status                                    # check daemon is alive
waos chat --account default unread             # see which chats need attention
waos memory --account default get <chat_id>    # read conversation context
waos message --account default send <chat_id> "Hello"   # respond
waos memory --account default set <chat_id> '{"notes":"..."}'   # save context
```

## Notes

- Output is JSON. Pipe to `jq` for filtering.
- Messages are queued — daemon sends them asynchronously.
- Media files must exist on the daemon's filesystem.
- Default account name is `default`.
- `chat history` returns each message with `is_me: true/false` — use this to distinguish agent replies from user messages.
- Outgoing messages are stored with `status: "sent"` on success, `"failed"` on failure. Receipts auto-update to `"delivered"`, `"read"`, etc.
- `chat history` includes `media_path` for media messages — the path to the file on the daemon's filesystem.
- `--text <caption>` on `media send` sets the caption on the actual WhatsApp media message (caption field on ImageMessage/VideoMessage/DocumentMessage). Without `--text`, no caption is attached.
- Media type is auto-detected from file extension: `.jpg/.png/.gif` → image, `.mp4/.mov` → video, `.mp3/.ogg` → audio, others → document.
- Use `--as-document` to force document type (bypass size limits: image/video/audio max 64MB, document max 100MB).
- If a file exceeds its media type size limit, the error message will suggest using `--as-document` or compressing.
- Status (stories) always goes to ALL contacts. Recipient list is read from the `contacts` table automatically.
- Status media size limits: images 10MB, videos 16MB. Stricter than regular media.
- Status requires a JPEG thumbnail. The agent must generate it using external tools (Python with Pillow, ffmpeg, etc.) and pass the path via `--thumbnail-file`.
- Status revocation: call `waos status revoke --account <name> <message_id>` with the `message_id` returned when the status was posted.
- All posted statuses are stored in `status_history` table with full metadata (media_path, thumbnail_path, bg_color, font, caption, message_id, timestamps).
- After 24 hours, statuses are considered expired (`is_expired` flag computed at query time). Use `waos status history --status expired` to see outdated statuses.
- The agent can re-upload old statuses using the stored `media_path` with a new caption: `waos status send-image --account default <media_path> --thumbnail-file <thumbnail_path> --caption "New caption"`. No need to re-download or regenerate media.
- Statuses stay in the DB until explicitly deleted via `waos status delete --status expired --older-than <TS> --yes`. Delete does NOT revoke from WhatsApp — use `revoke` first.
