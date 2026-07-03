# WAOS Agent CLI Reference

## Architecture

WAOS runs as a daemon (`waos daemon`). It exposes two interfaces:

1. **HTTP API** on `127.0.0.1:8111` (for web UI and curl)
2. **Unix IPC socket** at `{data_dir}/waos.sock` (for local CLI commands)

All CLI commands communicate with the daemon via the IPC socket. The daemon handles all database access — no lock contention.

## IPC Socket (Direct Access)

Python scripts can connect directly to read/write structured data:

```python
import socket, json

s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
s.connect("/path/to/srv/waos/waos.sock")
s.sendall(json.dumps({"cmd": "chat_unread", "account": "default"}).encode() + b"\n")
resp = json.loads(s.recv(65536).decode())
print(json.dumps(resp, indent=2))
s.close()
```

Send one JSON object per line. Receive one JSON line back (single command) or multiple lines (streaming commands).

---

## All CLI Commands

### `waos status`
List all accounts with their connection status.

**IPC equivalent:** `{"cmd": "status"}`
**Returns:** `{"accounts": [{"alias": "...", "phone": "...", "status": "...", "updated_at": ...}]}`

### `waos dashboard show`
Show the live dashboard JSON (accounts, unread counts, health).

**IPC equivalent:** `{"cmd": "dashboard"}`
**Returns:** The full dashboard JSON object.

### `waos dashboard watch`
Watch dashboard delta updates (streaming, polls `dashboard.delta` file). Falls back to direct file read (no IPC).

### `waos chat --account <name> list`
List all chats for an account, ordered by most recent.

**IPC equivalent:** `{"cmd": "chat_list", "account": "default"}`
**Returns:** `{"chats": [{"id": "...", "name": "...", "type": "...", "unread": N, "last_message": "...", "last_activity": N}]}`

### `waos chat --account <name> unread`
List only chats with unread messages.

**IPC equivalent:** `{"cmd": "chat_unread", "account": "default"}`

### `waos chat --account <name> search <query>`
Search messages across all chats.

**IPC equivalent:** `{"cmd": "chat_search", "account": "default", "query": "hello"}`
**Returns:** `{"results": [{"id": "...", "chat": "...", "sender": "...", "content": "..."}]}`

### `waos chat --account <name> history <chat_id> [--limit N] [--before TS] [--after TS] [--offset N] [--total]`
Show conversation history with `is_me` flag to distinguish agent vs user messages. Default: last 5. Paginate with `--before TS` (older), `--after TS` (newer), `--offset N` (skip). Use `--total` to include total message count.

**IPC equivalent:** `{"cmd": "chat_history", "account": "default", "chat_id": "...", "query": "5", "before": 1234567890, "total": true}`
**Returns:** `{"messages": [{"id": "...", "sender": "...", "content": "...", "timestamp": N, "type": "...", "has_media": bool, "media_path": "/path/to/file", "is_me": bool, "status": "sent|delivered|read|failed"}], "chat_id": "...", "limit": 5, "total": 42}`

### `waos chat --account <name> mark-read <chat_id>`
Mark a chat as read (set unread_count to 0).

**IPC equivalent:** `{"cmd": "chat_mark_read", "account": "default", "chat_id": "..."}`
**Returns:** `{"account": "default", "chat_id": "...", "action": "mark_read"}`

### `waos chat --account <name> listen <chat_id> [--timeout 120]`
Stream real-time events for a chat. Opens an IPC connection that stays open until timeout. Each line is one JSON event. Events include: `QueueItemStatusChanged`, `NewMessage`, `MessageReceipt`, `MessageEdited`, `MessageDeleted`, `ReactionAdded`, `ChatPresenceUpdate`.

**IPC equivalent:** `{"cmd": "chat_watch", "account": "default", "chat_id": "...", "timeout": 120}`
**Returns:** Multiple JSON lines (one per event). First line(s) are the pending queue snapshot. Example:
```
{"type":"queue_snapshot","account":"default","chat_id":"...","data":{"queue_id":"...","status":"pending","content":"Hello","created_at":123}}
{"type":"event","account":"default","chat_id":"...","message_id":"...","status":"sent",...}
```
Connection auto-closes after timeout or Ctrl+C.

### `waos message --account <name> send <chat_id> <text> [--wait <secs>]`
Queue a text message to send. With `--wait <secs>`, blocks until the message is sent or the timeout expires.

**IPC equivalent:** `{"cmd": "message_send", "account": "default", "chat_id": "...", "text": "Hello", "wait": true, "timeout": 30}`
**Returns (no wait):** `{"account": "default", "queue_id": "<uuid>", "action": "send"}`
**Returns (wait):** `{"queue_id": "...", "status": "sent", "message_id": "..."}` or `{"ok": false, "error": "timeout"}`

### `waos message --account <name> reply <chat_id> <reply_to_id> <text> [--wait <secs>]`
Queue a reply to a specific message. With `--wait <secs>`, blocks for confirmation.

**IPC equivalent:** `{"cmd": "message_reply", "account": "default", "chat_id": "...", "reply_to_id": "...", "text": "...", "wait": true, "timeout": 30}`

### `waos message --account <name> react <chat_id> <message_id> <emoji> [--wait <secs>]`
Queue a reaction to a message. With `--wait <secs>`, blocks for confirmation.

**IPC equivalent:** `{"cmd": "message_react", "account": "default", "chat_id": "...", "message_id": "...", "emoji": "👍", "wait": true, "timeout": 30}`

### `waos message --account <name> edit <chat_id> <message_id> <text> [--wait <secs>]`
Queue an edit of a sent message. With `--wait <secs>`, blocks for confirmation.

**IPC equivalent:** `{"cmd": "message_edit", "account": "default", "chat_id": "...", "message_id": "...", "text": "...", "wait": true, "timeout": 30}`

### `waos message --account <name> delete <chat_id> <message_id> [--wait <secs>]`
Queue deletion of a sent message. With `--wait <secs>`, blocks for confirmation.

**IPC equivalent:** `{"cmd": "message_delete", "account": "default", "chat_id": "...", "message_id": "...", "wait": true, "timeout": 30}`

### `waos media --account <name> send <chat_id> <file_path> [--text <caption>] [--reply-to <msg_id>] [--as-document] [--wait <secs>]`
Queue a media message. Media type is auto-detected from file extension (image/video/audio). Use `--as-document` to force send as document (for large/high-res files that exceed media type limits). With `--wait <secs>`, blocks for send confirmation.

**Size limits:** Images/videos/audio up to 64MB, documents up to 100MB. If exceeded, an error suggests using `--as-document` or compressing.

**IPC equivalent:** `{"cmd": "media_send", "account": "default", "chat_id": "...", "file_path": "/path/to/file.jpg", "text": "caption", "reply_to_id": "msg-id", "as_document": true, "wait": true, "timeout": 60}`
**Returns (no wait):** `{"account": "default", "queue_id": "<uuid>", "action": "media_send", "media_type": "image|video|audio|document", "file": "..."}`
**Returns (wait):** `{"queue_id": "...", "status": "sent", "message_id": "..."}` or `{"ok": false, "error": "timeout"}`

### `waos media --account <name> download <message_id>`
Look up media metadata for a message with media.

**IPC equivalent:** `{"cmd": "media_download", "account": "default", "message_id": "..."}`

### `waos contact --account <name> search <query>`
Search contacts by name, push_name, or phone number.

**IPC equivalent:** `{"cmd": "contact_search", "account": "default", "query": "..."}`
**Returns:** `{"contacts": [{"jid": "...", "name": "...", "phone": "...", "importance": 0.0, "interactions": N}]}`

### `waos contact --account <name> info <jid>`
Get detailed contact info including scores, conversation memory, and graph edges.

**IPC equivalent:** `{"cmd": "contact_info", "account": "default", "jid": "..."}`
**Returns:** Full contact profile with scores, memory, graph edges.

### `waos contact --account <name> score <jid> <value>`
Set a contact's importance score (0.0 to 1.0).

**IPC equivalent:** `{"cmd": "contact_score", "account": "default", "jid": "...", "score_value": 0.8}`
**Returns:** `{"account": "default", "jid": "...", "importance": 0.8}`

### `waos memory --account <name> get <chat_id>`
Get conversation memory for a chat.

**IPC equivalent:** `{"cmd": "memory_get", "account": "default", "chat_id": "..."}`
**Returns:** The stored memory JSON object (or default `{"notes": "", "topics": [], "pending": []}`).

### `waos memory --account <name> set <chat_id> <json>`
Set conversation memory for a chat. The JSON string is validated and stored.

**IPC equivalent:** `{"cmd": "memory_set", "account": "default", "chat_id": "...", "text": "{\"notes\": \"...\"}"}`

### `waos memory --account <name> search <query>`
Search memory across all chats. Uses vector search if embedding provider is configured, otherwise fallback LIKE search.

**IPC equivalent:** `{"cmd": "memory_search", "account": "default", "query": "..."}`

### `waos graph --account <name> shared-groups <jid1> <jid2>`
Find shared groups between two contacts.

**IPC equivalent:** `{"cmd": "graph_shared_groups", "account": "default", "jid1": "...", "jid2": "..."}`

### `waos graph --account <name> introductions <jid>`
Find potential introduction paths for a contact.

**IPC equivalent:** `{"cmd": "graph_introductions", "account": "default", "jid": "..."}`

### `waos graph --account <name> important-contacts <community_id>`
Get important contacts in a community.

**IPC equivalent:** `{"cmd": "graph_important_contacts", "account": "default", "community_id": "..."}`

### `waos community --account <name> list`
List all communities for an account.

**IPC equivalent:** `{"cmd": "community_list", "account": "default"}`
**Returns:** `{"communities": [{"id": "...", "name": "..."}]}`

### `waos community --account <name> info <community_id>`
Get community details including member list.

**IPC equivalent:** `{"cmd": "community_info", "account": "default", "community_id": "..."}`

### `waos status list`
List all accounts with their connection status (same as `waos status`).

**IPC equivalent:** `{"cmd": "status"}`
**Returns:** `{"accounts": [{"alias": "...", "phone": "...", "status": "...", "updated_at": ...}]}`

### `waos status send-text --account <name> <text> [--bg-color <hex>] [--font <n>] [--wait <secs>]`
Post a text status/story visible to all contacts. Background color as 0xAARRGGBB hex (default `0xFF1E6E4F`), font 0-4 (default 0). With `--wait <secs>`, blocks until the daemon confirms send.

**IPC equivalent:** `{"cmd": "status_send_text", "account": "default", "text": "Hello!", "metadata": "{\"bg\":\"0xFF1E6E4F\",\"font\":0}", "wait": true, "timeout": 30}`
**Returns (no wait):** `{"account": "default", "queue_id": "<uuid>", "action": "status_send_text"}`
**Returns (wait):** `{"queue_id": "...", "status": "sent", "message_id": "..."}` or error

### `waos status send-image --account <name> <file_path> --thumbnail-file <thumb.jpg> [--caption <text>] [--wait <secs>]`
Post an image status/story. `--thumbnail-file` is required — the agent must generate a JPEG thumbnail using external tools (Python with Pillow, ffmpeg, etc.). Sent to all contacts. With `--wait <secs>`, blocks for confirmation.

**IPC equivalent:** `{"cmd": "status_send_image", "account": "default", "file_path": "/path/to/image.jpg", "text": "caption", "metadata": "{\"thumbnail\":\"/path/to/thumb.jpg\"}", "wait": true, "timeout": 30}`
**Returns (no wait):** `{"account": "default", "queue_id": "<uuid>", "action": "status_image"}`

### `waos status send-video --account <name> <file_path> --thumbnail-file <thumb.jpg> [--caption <text>] [--wait <secs>]`
Post a video status/story. Same requirements as send-image. Sent to all contacts.

**IPC equivalent:** `{"cmd": "status_send_video", "account": "default", "file_path": "/path/to/video.mp4", "text": "caption", "metadata": "{\"thumbnail\":\"/path/to/thumb.jpg\"}", "wait": true, "timeout": 30}`

### `waos status revoke --account <name> <message_id> [--wait <secs>]`
Delete a previously posted status. The `message_id` is the one returned when the status was posted. Sent to all contacts.

**IPC equivalent:** `{"cmd": "status_revoke", "account": "default", "message_id": "<id>", "wait": true, "timeout": 30}`
**Returns (no wait):** `{"account": "default", "queue_id": "<uuid>", "action": "status_revoke"}`

### `waos status history --account <name> [--status <filter>] [--before TS] [--after TS] [--limit N] [--offset N]`
List posted status history with date range and pagination. Default limit is 5 to avoid bloating agent context.

**Filters:**
- `--status` one of: `posted`, `revoked`, `expired`, `all` (default: `all`). `expired` filters to statuses older than 24h.
- `--before TS` — only statuses older than this timestamp
- `--after TS` — only statuses newer than this timestamp
- `--limit N` — max results (default: 5)
- `--offset N` — skip N results for pagination

Each entry includes `is_expired` (computed at query time), `media_path`, `thumbnail_path`, `bg_color`, `font`, `message_id` — agent can re-upload old media with new caption using the stored paths.

**IPC equivalent:** `{"cmd": "status_history", "account": "default", "query": "all", "text": "5", "reply_to_id": "0"}`
**Returns:** `{"account": "default", "statuses": [...], "total": 12, "limit": 5, "offset": 0}`

### `waos status delete --account <name> [--status <filter>] [--older-than TS] [--newer-than TS] [--yes]`
Delete status history entries by date range. **DB cleanup only** — does NOT revoke from WhatsApp. Use `status revoke` first to remove from WhatsApp.

**Dry-run by default.** Add `--yes` to actually delete.

**IPC equivalent:** `{"cmd": "status_delete", "account": "default", "query": "expired", "text": "yes"}`
**Returns (dry-run):** `{"account": "default", "dry_run": true, "count": 3, "message": "3 entries would be deleted. Set text='yes' to confirm"}`
**Returns (actual):** `{"account": "default", "deleted": 3}`

### `waos task list [--status <status>]`
List tasks, optionally filtered by status (e.g. "pending", "completed").

**IPC equivalent:** `{"cmd": "task_list"}` or `{"cmd": "task_list", "status": "pending"}`
**Returns:** `{"tasks": [{"id": N, "account": "...", "action": "...", "params": "...", "status": "...", "created_at": N}]}`

### `waos task complete <task_id>`
Mark a task as completed.

**IPC equivalent:** `{"cmd": "task_complete", "query": "123"}`
**Returns:** `{"task_id": 123}`

---

## Common Agent Workflows

### Post a status/story

```python
import socket, json

def ipc(cmd):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect("/path/to/srv/waos/waos.sock")
    s.sendall(json.dumps(cmd).encode() + b"\n")
    resp = json.loads(s.recv(65536).decode())
    s.close()
    return resp

# 1. Generate thumbnail using Python/Pillow (required for media status)
#    from PIL import Image
#    img = Image.open("image.jpg")
#    img.thumbnail((100, 100))
#    img.save("thumb.jpg", "JPEG")

# 2. Post text status (no thumbnail needed)
ipc({"cmd": "status_send_text", "account": "default",
     "text": "Hello from AI agent!", "metadata": '{"bg":"0xFF1E6E4F","font":0}'})

# 3. Post image status (thumbnail required)
ipc({"cmd": "status_send_image", "account": "default",
     "file_path": "/path/to/image.jpg",
     "metadata": '{"thumbnail":"/path/to/thumb.jpg"}',
     "text": "Check this out!", "wait": true, "timeout": 30})
```

### Check for new messages and respond

```python
import socket, json

def ipc(cmd):
    s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    s.connect("/path/to/srv/waos/waos.sock")
    s.sendall(json.dumps(cmd).encode() + b"\n")
    resp = json.loads(s.recv(65536).decode())
    s.close()
    return resp

# 1. Check unread chats
unread = ipc({"cmd": "chat_unread", "account": "default"})
for chat in unread.get("data", {}).get("chats", []):
    print(f"Unread: {chat['name']} ({chat['unread']})")

# 2. Send a reply
ipc({"cmd": "message_send", "account": "default",
     "chat_id": "123@s.whatsapp.net", "text": "Hello from AI agent!"})

# 3. Check status
status = ipc({"cmd": "status"})
```

### Read and update contact memory

```python
# Get existing memory
mem = ipc({"cmd": "memory_get", "account": "default", "chat_id": "123@s.whatsapp.net"})
notes = mem.get("data", {}).get("notes", "")

# Update memory with new context
ipc({"cmd": "memory_set", "account": "default", "chat_id": "123@s.whatsapp.net",
     "text": '{"notes": "User prefers short replies", "topics": ["tech"], "pending": []}'})
```

---

## Response Format

All IPC responses follow this structure:

```json
{"ok": true, "data": { ... }}
{"ok": false, "error": "error message"}
```

When `ok` is `false`, the `error` field contains the error description.

## Message Status

Outgoing messages are stored in `messages` table with a `status` column:
- `"sent"` — successfully sent to WhatsApp
- `"failed"` — send error (network, rate-limit, etc.) — the agent should retry
- `"delivered"` — delivered to recipient's device (from WhatsApp receipt)
- `"read"` — read by recipient (from WhatsApp read receipt)

Use `chat history` to check message status via `is_me: true` messages' `status` field in the raw JSON output.
- `chat history` includes `media_path` for media messages showing the server-side file path.
- `--text <caption>` sets the caption on the media message (ImageMessage.caption, VideoMessage.caption, DocumentMessage.caption). Previously the caption was only stored in DB but not sent on the wire.

## Status Events

The daemon publishes `StatusUpdate` events when a status item is processed:
- `{"StatusUpdate": {"account": "...", "status_id": "...", "action": "posted|revoked", "message_id": "...", "timestamp": N}}`

## Status Media Limits

- Image status: max 10MB
- Video status: max 16MB
- Thumbnail: required, must be a JPEG file. Generate via external tools (Python/Pillow, ffmpeg, etc.). Not generated by WAOS.

waos group --account <name> list           # id, name, type, enabled from wa_groups
waos group --account <name> enable <jid>   # enabled=1 → messages get stored
waos group --account <name> disable <jid>  # enabled=0 → messages ignored

waos group --account <name> mentions                 # all group messages that tagged you
waos group --account <name> mentions <group_jid>      # only in that group
waos group --account <name> mentions --limit 50       # more results

waos group --account <name> replies                   # all replies to your messages
waos group --account <name> replies <group_jid>       # only in that group
