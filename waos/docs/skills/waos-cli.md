---
name: waos-cli
description: WhatsApp Agent Operating System — headless WhatsApp for AI agents
---

# WAOS CLI for Agents

## Command Categories

| Category | Description |
|---|---|
| `status` | Account status, daemon health |
| `chat` | List, search, history, listen |
| `message` | Send, reply, react, edit, delete |
| `media` | Send media, download |
| `contact` | Search, info, score |
| `memory` | Get, set, search conversation memory |
| `graph` | Shared groups, introductions, contacts |
| `community` | List, info |
| `stories` | Post text/image/video status, revoke, history |
| `task` | List, complete |
| `dashboard` | Show, watch |

```
waos help <category>    show all commands in a category
waos <category> --help  same
```

## `--wait` flag (all commands)

```
--wait        wait 30s for result (default when absent on queue commands)
--wait 60     wait 60s
--wait 0      return immediately (opt out)
```

Queue commands (message send, media send, status post) insert into an outbound queue and return a `queue_id`. With `--wait`, terminal blocks until daemon confirms send. On timeout, connection drops but command continues on daemon. Query commands (list, search, history) return instantly regardless.

## waos status

```
waos status                                          list accounts with connection status
waos daemon                                          start daemon (bots, API, IPC socket)
waos pair <alias>                                    show pairing instructions for a new phone
```

## waos chat --account <account>

```
list                                                 all chats, most recent first
unread                                               only chats with unread messages
search <query>                                       search messages across all chats
history <chat_id>                                    last 5 messages (has is_me flag)
history <chat_id> --limit N                          increase page size
history <chat_id> --before <ts>                      paginate older (DESC)
history <chat_id> --after <ts>                       paginate newer (ASC)
history <chat_id> --offset N --limit N               skip + take
history <chat_id> --total                            include total count
mark-read <chat_id>                                  mark chat as read
listen <chat_id>                                     stream events (--timeout N, default 120s)

# usage
waos chat --account <account> list
waos chat --account <account> unread
waos chat --account <account> search "<query>"
waos chat --account <account> history <chat_id>
waos chat --account <account> history <chat_id> --limit 20 --before 1700000000
waos chat --account <account> mark-read <chat_id>
waos chat --account <account> listen <chat_id>
```

Output: each message has `id`, `sender`, `content`, `timestamp`, `type`, `has_media`, `media_path`, `is_me`, `status`. Use `is_me` to distinguish agent replies from user messages.

## waos message --account <account>

```
send <chat_id> "<text>"                              queue a text message
reply <chat_id> <reply_to_id> "<text>"               queue a reply
react <chat_id> <message_id> "<emoji>"               queue a reaction
edit <chat_id> <message_id> "<new text>"             queue an edit
delete <chat_id> <message_id>                        queue a delete

# usage
waos message --account <account> send <chat_id> "Hello"
waos message --account <account> reply <chat_id> <msg_id> "Thanks!"
waos message --account <account> react <chat_id> <msg_id> "👍"
waos message --account <account> edit <chat_id> <msg_id> "corrected text"
waos message --account <account> delete <chat_id> <msg_id>
```

Returns `queue_id`. Outgoing messages store `status: "sent"` on success, `"failed"` on failure.

## waos media --account <account>

Media files must exist on the daemon's filesystem (not on the agent's machine).

```
send <chat_id> <path/to/file>                        auto-detect: .jpg/.png/.gif→image, .mp4/.mov→video, .mp3/.ogg→audio, other→document
send <chat_id> <path/to/file> --text "caption"       attach text caption
send <chat_id> <path/to/file> --reply-to <msg_id>    reply with media
send <chat_id> <path/to/file> --as-document          force document type (bypasses 64MB, max 100MB)
download <message_id>                                lookup media metadata

# usage
waos media --account <account> send <chat_id> <path/to/image.jpg>
waos media --account <account> send <chat_id> <path/to/video.mp4> --text "Look at this"
waos media --account <account> send <chat_id> <path/to/file.pdf> --as-document
waos media --account <account> download <message_id>
```

Size limits: image/video/audio 64MB, document 100MB. If exceeded, error suggests `--as-document` or compress.

## waos contact --account <account>

```
search <query>                                       search by name or phone
info <jid>                                           full profile: scores, memory, graph
score <jid> <0.0-1.0>                                set importance score (auto-clamped)

# usage
waos contact --account <account> search "John"
waos contact --account <account> info <jid>
waos contact --account <account> score <jid> 0.85
```

## waos memory --account <account>

```
get <chat_id>                                        read stored conversation memory (JSON)
set <chat_id> '<json>'                               store JSON memory for a chat
search "<query>"                                     search memory across chats (vector or LIKE)

# usage
waos memory --account <account> get <chat_id>
waos memory --account <account> set <chat_id> '{"notes":"customer wants refund","topics":["billing"]}'
waos memory --account <account> search "billing issue"
```

## waos graph --account <account>

```
shared-groups <jid1> <jid2>                          find shared groups between two contacts
introductions <jid>                                  find introduction paths to reach a contact
important-contacts <community_id>                    key contacts in a community

# usage
waos graph --account <account> shared-groups <jid1> <jid2>
waos graph --account <account> introductions <jid>
waos graph --account <account> important-contacts <community_id>
```

## waos community --account <account>

```
list                                                 list all communities
info <community_id>                                  community details + members

# usage
waos community --account <account> list
waos community --account <account> info <community_id>
```

## waos status (Stories)

Stories are posted to ALL contacts. Recipients from `contacts` table. Media files on daemon filesystem.

```
send-text --account <account> "<text>"               post text story
         --bg-color 0xAARRGGBB (default 0xFF1E6E4F)
         --font 0-4 (default 0)

send-image --account <account> <path/to/image.jpg>   post image story. Thumbnail required (JPEG, Pillow/ffmpeg)
         --thumbnail-file <path/to/thumb.jpg>
         --caption "text"

send-video --account <account> <path/to/video.mp4>   post video story. Thumbnail required.
         --thumbnail-file <path/to/thumb.jpg>
         --caption "text"

revoke --account <account> <message_id>              delete a posted story from WhatsApp
history --account <account>                          list posted stories (--limit N, --before/--after TS, --offset N, --status expired)
delete --account <account>                           delete from DB (does NOT revoke — use revoke first)
         --status posted|revoked|expired|all
         --older-than TS --newer-than TS
         --yes (dry-run by default)

# usage
waos status send-text --account <account> "Hello world" --bg-color 0xFF1E6E4F --font 2
waos status send-image --account <account> <path/to/photo.jpg> --thumbnail-file <path/to/thumb.jpg> --caption "Sunset"
waos status send-video --account <account> <path/to/clip.mp4> --thumbnail-file <path/to/thumb.jpg>
waos status revoke --account <account> <message_id>
waos status history --account <account> --limit 10 --status expired
waos status delete --account <account> --status expired --older-than 1700000000 --yes
```

Size limits: images 10MB, videos 16MB. Each history entry has `media_path`, `thumbnail_path`, `message_id`, `is_expired` for re-upload with new caption.

## waos task

```
list                                                list all tasks
list --status <status>                              filter by status (pending/completed)
complete <task_id>                                  mark task completed

# usage
waos task list
waos task list --status pending
waos task complete 42
```

## waos dashboard

```
show                                                show live dashboard JSON (accounts, unread, health)
watch                                               stream dashboard updates live

# usage
waos dashboard show
waos dashboard watch
```

## Agent Workflow

```
waos status                                       check daemon is alive
waos chat --account <account> unread              see which chats need attention
waos memory --account <account> get <chat>        read conversation context
waos message --account <account> send ...         respond
waos memory --account <account> set <chat> '{"notes":"..."}'  save context
```

## waos group

```
waos group --account <account> list           # list groups
waos group --account <account> enable <jid>   # enable messages for a group
waos group --account <account> disable <jid>  # disable messages for a group

waos group --account <name> mentions                 # all group messages that tagged you
waos group --account <name> mentions <group_jid>      # only in that group
waos group --account <name> mentions --limit 50       # more results

waos group --account <name> replies                   # all replies to your messages
waos group --account <name> replies <group_jid>       # only in that group


```

## Output

All output is JSON. Pipe to `jq` for filtering. Each message in `chat history` includes `is_me: true/false`. Media messages include `media_path` (path on daemon's filesystem).
