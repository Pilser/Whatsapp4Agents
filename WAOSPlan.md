# WhatsApp Agent Operating System (WAOS)

## Vision

Build a headless WhatsApp platform that allows AI agents to operate as first-class WhatsApp participants.

Unlike traditional chatbot systems that only respond to incoming messages, this platform provides a complete digital representation of a WhatsApp account, enabling AI agents to:

- View chats
- Read message history
- Detect unread conversations
- Monitor notifications
- Participate in groups
- Manage communities
- Post statuses
- React to messages
- Search conversations
- Track relationships
- Manage tasks and follow-ups

The AI agent perceives WhatsApp similarly to how a human user experiences it, but through structured tools and a live state file rather than a graphical user interface.

---

## Core Principle

The AI must never interact directly with WhatsApp.

```
WhatsApp (multiple accounts)
    ↓
wa-rs Adapter (one per account)
    ↓
Bridge Layer (dedup, anti-ban, queue)
    ↓
Synchronization Engine
    ↓
Internal WhatsApp Replica (SQLite, per-account or unified)
    ↓
Event Bus + Agent Dashboard File (aggregated across accounts)
    ↓
Agent Tools (CLI, --account flag for targeting)
    ↓
AI Agents
```

The internal replica becomes the agent's reality.

---

## System Objectives

### Functional Goals

Support:

- Direct messages
- Groups
- Communities
- Channels
- Statuses
- Media messages (images, video, audio, documents)
- Reactions
- Mentions
- Polls
- Notifications

Allow agents to:

- Read conversations
- Search history
- Reply to messages
- Send media
- React
- Create status posts
- Track relationships
- Manage ongoing conversations
- Prioritize attention
- Maintain conversation memory

---

## High-Level Architecture

```
┌──────────────────────────────────┐
│ WhatsApp Network                  │
└──┬───────────────┬────────────┬──┘
   │               │            │
   ▼               ▼            ▼
┌──────────┐ ┌──────────┐ ┌──────────┐
│ wa-rs    │ │ wa-rs    │ │ wa-rs    │
│ Account 1│ │ Account 2│ │ Account N│
│ (alias:  │ │ (alias:  │ │ (alias:  │
│ default) │ │ business)│ │ personal)│
└─────┬────┘ └─────┬────┘ └─────┬────┘
      │            │            │
      └────────────┼────────────┘
                   ▼
┌──────────────────────────────┐
│ Bridge Layer                 │
│ (dedup, anti-ban, queue)     │
└──────┬───────────────┬───────┘
       │               │
       ▼               ▼
┌──────────────┐  ┌──────────────────┐
│ Event Bus    │  │ WA Database      │
│ (broadcast)  │  │ (SQLite replica  │
│              │  │  per-account or  │
│              │  │  unified)        │
└──────┬───────┘  └────────┬─────────┘
       │                   │
       ▼                   ▼
┌──────────────────────────────────┐
│ Synchronization Engine           │
│ (rebuild state after restart)    │
└──────┬───────────────────┬───────┘
       │                   │
       ▼                   ▼
┌──────────────────┐  ┌──────────────────┐
│ Attention Engine │  │ Memory Layer     │
│ (priority queue) │  │ (per-conversation│
└──────┬───────────┘  │  notes)          │
       │              └────────┬─────────┘
       ▼                       │
┌──────────────────────────────┐
│ Agent State File (dashboard) │
│ ─ live snapshot for agent    │
│ ─ includes ALL accounts      │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ Agent Tool Layer (CLI)       │
│ ─ executable subcommands     │
│ ─ JSON output                │
│ ─ --account flag for multi-  │
│   account operations         │
└──────────────┬───────────────┘
               │
               ▼
┌──────────────────────────────┐
│ AI Agents                    │
│ (triggered by notifications │
│  or polling dashboard file)  │
└──────────────────────────────┘
```

## Agent Trigger Architecture

The agent and WAOS are decoupled. WAOS never waits for the agent. Two complementary trigger methods ensure the agent never misses activity.

### Method 1: Webhook Push (immediate trigger)

WAOS acts as an HTTP client (no server needed). On every high-priority event, it POSTs a lightweight notification to zeroclaw's gateway.

```
WAOS detects message
  → POST http://localhost:9090/webhook {"message": "new_activity"}
  → zeroclaw gateway receives (already running, persistent session)
  → agent processes via LLM + tools
```

- **WAOS is pure HTTP client** — uses `reqwest` / `curl` / `ureq`. No Axum, no hyper.
- **Zeroclaw gateway is persistent** — started once, stays alive. No new session per trigger.
- **Notification is lightweight** — just a bell ring. Agent reads dashboard.json for full context.
- **No lost triggers** — if agent is busy, the POST succeeds and dashboard.json has the full state waiting.

### Method 2: Dashboard Polling (guaranteed catch-up)

The dashboard file is always up to date regardless of webhook delivery. The agent can read it at any time.

```
Agent startup:
  1. Read ./srv/waos/dashboard.json
  2. See full attention_queue with everything missed
  3. Process pending items

Agent periodic check:
  waos dashboard      ← returns full JSON state
  waos dashboard watch ← blocks and streams deltas
```

- **Zero HTTP** — agent just reads a local file or runs a CLI command.
- **No message loss** — every event is in the dashboard. Agent catches up on restart.
- **Combined with webhook** — webhook provides immediate trigger, dashboard provides state recovery.

### Dual Path Guarantee

```
Event occurs
  ↓
WAOS writes dashboard.json (always, synchronous)
WAOS POSTs HTTP webhook (async, best-effort)
  ↓
If agent is running:     webhook arrives → agent reads dashboard → acts
If agent is busy:        webhook maybe lost → dashboard has it → agent gets it on next poll
If agent is down:        webhook lost → agent starts → reads dashboard → catches up
```

The dashboard is the **source of truth**. The webhook is a **notification shortcut**. Both together mean zero message loss.

---

## Component 1: WhatsApp Adapter

### Purpose

Responsible for communicating with WhatsApp through the Rust library.

### Implementation

Uses `whatsapp-rust` v0.6+ library directly:

```rust
use whatsapp_rust::prelude::*;

let bot = Bot::builder()
    .with_backend(SqliteStore::new("whatsapp.db").await?)
    .on_qr_code(|code, _timeout| async move {
        println!("QR:\n{}", code);
    })
    .on_event(|event, client| async move {
        match event {
            Event::Message(msg, info) => { /* forward to bridge */ }
            Event::Reaction { .. } => { /* forward to bridge */ }
            _ => {}
        }
    })
    .build()
    .await?;

bot.run().await;
```

### Responsibilities

- Authentication (QR pairing + pair code)
- Session persistence and reconnection
- E2E encrypted messaging (Signal Protocol)
- Media upload/download
- Receiving events from WhatsApp
- Sending messages on behalf of bridge
- Presence and typing indicators

### Multi-Account Support

WAOS supports multiple WhatsApp accounts managed by a single daemon. Each account gets its own independent wa-rs Bot instance (separate WebSocket connection, session DB, and Signal protocol keys) but shares the bridge, event bus, and dashboard.

#### Configuration

Accounts are configured by alias in `config.toml`:

```toml
[accounts.default]
session_path = "./srv/waos/sessions/default/"
phone = "15551234567"
enabled = true

[accounts.business]
session_path = "./srv/waos/sessions/business/"
phone = "15559876543"
pair_code = "ABCD1234"
enabled = true

[accounts.personal]
session_path = "./srv/waos/sessions/personal/"
phone = "15555550000"
enabled = false  # can be activated later
```

#### Per-Account Isolation

- Each account has its own `session_path` for Signal protocol keys and session state
- Each account maintains its own WebSocket connection to WhatsApp
- Account data is tagged with `account_alias` in the replica database (either per-account DB files or a unified DB with an `account` column)
- The dashboard file includes an `accounts` key that aggregates across all active accounts

#### CLI Account Targeting

CLI commands accept an `--account` flag to target a specific account:

```
waos --account business chat list
waos --account default message send "123456789@s.whatsapp.net" "Hello"
waos --account personal dashboard
```

Without `--account`, commands operate on the default account or all accounts (for read-only queries like `dashboard`).

#### Unified Dashboard

The dashboard file shows all accounts:

```json
{
  "accounts": {
    "default": { "connection": "connected", "unread": 5, "phone": "15551234567" },
    "business": { "connection": "connected", "unread": 12, "phone": "15559876543" }
  },
  "summary": {
    "total_unread": 17,
    "total_accounts": 2,
    "total_connected": 2
  },
  "attention_queue": [
    {
      "account": "business",
      "rank": 1,
      "chat_name": "Client ABC",
      "reason": "Urgent: pricing request"
    }
  ]
}
```

### Design Rule

No business logic lives here. The adapter only converts WhatsApp protocol data into internal events.

---

## Component 2: Bridge Layer

### Purpose

Sits between raw `whatsapp-rust` events and the rest of WAOS. Handles all cross-cutting concerns that keep the account healthy and the system reliable.

### Responsibilities

#### Message Deduplication

WhatsApp can deliver the same event multiple times. The bridge maintains a generation-tracked map of seen message IDs and discards duplicates before they reach the event bus.

#### Anti-Ban Safety

Meta bans accounts that behave like bots. The bridge enforces:

- **Send pacing** — minimum delay between outgoing messages (configurable, default 800ms–1.5s with jitter)
- **Read receipt batching** — don't mark-as-read per-message. Batch every 3–10 seconds or when user switches chats.
- **Typing simulation** — when sending messages automatically, optionally simulate "composing..." for realistic delay.
- **Daily caps** — configurable max sends per day/hour to avoid triggering rate limits.
- **Jitter** — all timers have random variance so behavior isn't perfectly periodic.

#### Durable Outbound Queue

Messages to send go into a SQLite queue first. The bridge picks them up, sends them, and marks them delivered/failed. This means:

- Crashes don't lose messages
- Rate limits are respected globally
- Send order is preserved per-chat

#### Media Download Cache

Automatically downloads and caches incoming media to disk. Provides local file paths to the rest of the system so agents don't need to handle encrypted blobs.

#### Crash Recovery

On restart, the bridge:

1. Reconnects to WhatsApp (persistent session)
2. Replays any pending outbound messages from the queue
3. Processes any pending read receipt batches
4. Signals the sync engine to catch up on missed events

---

## Component 3: WhatsApp Replica Database

### Purpose

A complete digital twin of the WhatsApp account backed by SQLite.

### Schema

#### `chats`

```sql
CREATE TABLE chats (
    id           TEXT PRIMARY KEY,
    name         TEXT,
    type         TEXT NOT NULL,  -- 'direct' | 'group' | 'community' | 'channel' | 'system'
    last_message TEXT,
    last_activity INTEGER,       -- unix timestamp
    unread_count INTEGER DEFAULT 0,
    is_archived  INTEGER DEFAULT 0,
    is_pinned    INTEGER DEFAULT 0,
    is_muted     INTEGER DEFAULT 0,
    created_at   INTEGER
);
```

#### `messages`

```sql
CREATE TABLE messages (
    id            TEXT PRIMARY KEY,
    chat_id       TEXT NOT NULL REFERENCES chats(id),
    sender_jid    TEXT,
    sender_name   TEXT,
    content       TEXT,
    media_type    TEXT,           -- 'image' | 'video' | 'audio' | 'document' | 'sticker' | null
    media_path    TEXT,           -- local file path if downloaded
    reply_to_id   TEXT,
    mentions      TEXT,           -- JSON array of mentioned JIDs
    reactions     TEXT,           -- JSON object { emoji: [sender_jid, ...] }
    delivery_state TEXT,          -- 'sent' | 'delivered' | 'read' | 'failed'
    is_edited     INTEGER DEFAULT 0,
    is_deleted    INTEGER DEFAULT 0,
    timestamp     INTEGER NOT NULL
);
```

#### `contacts`

```sql
CREATE TABLE contacts (
    jid              TEXT PRIMARY KEY,
    display_name     TEXT,
    phone            TEXT,
    avatar_path      TEXT,
    last_seen        INTEGER,
    about            TEXT,
    is_business      INTEGER DEFAULT 0,
    relationship_score REAL DEFAULT 0.5,
    notes            TEXT,         -- agent-written memory
    created_at       INTEGER,
    updated_at       INTEGER
);
```

#### `groups`

```sql
CREATE TABLE groups (
    id          TEXT PRIMARY KEY,
    name        TEXT,
    description TEXT,
    community_id TEXT,
    member_count INTEGER,
    admin_jids  TEXT,             -- JSON array
    is_join_approval INTEGER DEFAULT 0,
    created_at  INTEGER
);

CREATE TABLE group_members (
    group_id TEXT NOT NULL,
    jid      TEXT NOT NULL,
    role     TEXT DEFAULT 'member', -- 'admin' | 'member'
    added_at INTEGER,
    PRIMARY KEY (group_id, jid)
);
```

#### `communities`

```sql
CREATE TABLE communities (
    id              TEXT PRIMARY KEY,
    name            TEXT,
    announcement_group_id TEXT,
    linked_groups   TEXT,         -- JSON array of group IDs
    created_at      INTEGER
);
```

#### `statuses`

```sql
CREATE TABLE statuses (
    id          TEXT PRIMARY KEY,
    owner_jid   TEXT,
    content     TEXT,
    media_type  TEXT,
    media_path  TEXT,
    timestamp   INTEGER,
    view_count  INTEGER DEFAULT 0,
    is_deleted  INTEGER DEFAULT 0
);
```

#### `memory`

```sql
CREATE TABLE memory (
    conversation_id TEXT PRIMARY KEY,
    notes           TEXT,         -- free-form agent notes
    topics          TEXT,         -- JSON array of discussed topics
    pending_items   TEXT,         -- JSON array of follow-ups
    last_updated    INTEGER
);
```

#### `outbound_queue`

```sql
CREATE TABLE outbound_queue (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id     TEXT NOT NULL,
    message     TEXT,
    media_type  TEXT,
    media_path  TEXT,
    reply_to_id TEXT,
    status      TEXT DEFAULT 'pending',  -- 'pending' | 'sending' | 'sent' | 'failed'
    created_at  INTEGER,
    sent_at     INTEGER,
    error       TEXT
);
```

### Guarantees

- **Durability** — all data survives restart
- **Rebuildable** — sync engine can re-fetch history from WhatsApp
- **Queryable** — agents can search messages, filter by chat, filter by sender, etc.

---

## Component 4: Event Bus

### Purpose

Convert WhatsApp activity into structured events that flow through the system.

### Implementation

Uses `tokio::sync::broadcast` with a configurable buffer.

```rust
#[derive(Clone, Debug)]
pub enum WaosEvent {
    // Messages
    NewMessage {
        chat_id: String,
        message_id: String,
        sender_jid: String,
        sender_name: String,
        content: String,
        has_mentions: Vec<String>,
        has_media: bool,
        timestamp: i64,
    },
    MessageEdited {
        chat_id: String,
        message_id: String,
        new_content: String,
    },
    MessageDeleted {
        chat_id: String,
        message_id: String,
    },

    // Reactions
    ReactionAdded {
        chat_id: String,
        message_id: String,
        emoji: String,
        sender_jid: String,
    },
    ReactionRemoved {
        chat_id: String,
        message_id: String,
        emoji: String,
        sender_jid: String,
    },

    // Mentions (computed by bridge, not raw)
    MentionReceived {
        chat_id: String,
        chat_name: String,
        sender_name: String,
        message_snippet: String,
        timestamp: i64,
    },

    // Groups / Communities
    UserJoinedGroup {
        group_id: String,
        group_name: String,
        user_jid: String,
        user_name: String,
    },
    UserLeftGroup {
        group_id: String,
        user_jid: String,
    },
    CommunityAnnouncement {
        community_id: String,
        content: String,
    },

    // Status
    StatusPublished {
        status_id: String,
        owner_jid: String,
        owner_name: String,
        content: String,
    },

    // Presence
    UserPresenceChanged {
        jid: String,
        presence: String, // 'online' | 'offline' | 'typing' | 'recording'
    },

    // Connection
    Connected,
    Disconnected,
    Reconnecting,
    QrCodeReady { code: String },
}
```

### Subscribers

1. **Synchronization Engine** — stores every event into SQLite
2. **Attention Engine** — scores events, queues attention items
3. **Agent Dashboard** — updates the live state file on every event
4. **Notification Dispatcher** — emits OS notifications for high-priority events

---

## Component 5: Notification System

### Purpose

Recreate the human notification experience so the AI agent knows when to act.

### Behavior

Not every event triggers a notification. The notification system applies rules:

| Event Type | Notify? | Priority |
|---|---|---|
| Direct message from known contact | Yes | High |
| Mention (@agent) in any group | Yes | Critical |
| Message from client (scored contact) | Yes | High |
| Message in important group | Conditional | Medium |
| Community announcement | Conditional | Medium |
| Status published | No | Low |
| Presence change | No | Ignore |
| Group join/leave | Conditional | Low |

### OS Notification Dispatch

```
notify-send \
  --urgency=critical \
  --app-name="WAOS" \
  --category="whatsapp" \
  "John: Hey, can you send the quote?" \
  "Chat: John Doe | Unread: 3 | Priority: 0.92"
```

The AI agent picks this up via its notification listener, reads the dashboard file to get full context, and decides whether to act.

### Priority Scoring

```
direct_message        → 1.00
mention               → 0.95
high_value_contact    → 0.90
important_group       → 0.85
community_announce    → 0.70
general_activity      → 0.40
status                → 0.20
```

The score decays over time if not acknowledged.

---

## Component 6: Attention Engine

### Problem

An account may contain thousands of chats, hundreds of groups, and tens of thousands of messages. An AI cannot process everything.

### Purpose

Determine what deserves attention, what can wait, and what should be ignored.

### Inputs

- Unread counts per chat
- Mentions (already parsed by bridge)
- Contact importance score (from relationship graph)
- Conversation timestamps (recency)
- Agent goals / task deadlines (from memory layer)
- Notification priority scores

### Algorithm

```
attention_score(chat) =
    (unread_weight * min(unread, 10) / 10) +
    (mention_weight * has_mention) +
    (recency_weight * recency_factor) +
    (importance_weight * contact_importance) +
    (goal_weight * goal_relevance) +
    (notification_priority * 0.2)

sort by attention_score descending
```

### Output

A prioritized list rendered into the agent dashboard file:

```json
{
  "attention_queue": [
    {
      "rank": 1,
      "chat_id": "123456789@s.whatsapp.net",
      "chat_name": "John Doe",
      "type": "direct",
      "reason": "2 unread messages, high importance contact",
      "score": 0.92,
      "summary": "Asking about Rust project proposal"
    },
    {
      "rank": 2,
      "chat_id": "987654321@g.us",
      "chat_name": "Dev Team",
      "type": "group",
      "reason": "Agent mentioned by @alice",
      "score": 0.88,
      "summary": "Discussion about deployment timeline"
    }
  ]
}
```

### Boundary

The attention engine does NOT decide what the agent does. It only provides a recommendation. The agent makes the final decision.

---

## Component 7: Agent Tool Layer (CLI)

### Purpose

The primary interface exposed to AI agents. Every tool is a CLI subcommand that returns structured JSON.

### Design

Single binary `waos` with subcommands:

```
waos chat list              → JSON array of all chats
waos chat open <id>         → JSON with chat metadata + last N messages
waos chat unread            → JSON array of chats with unread > 0

waos message send <chat> <text>
waos message reply <chat> <msg-id> <text>
waos message react <chat> <msg-id> <emoji>
waos message edit <chat> <msg-id> <text>
waos message delete <chat> <msg-id>
waos message search <query>

waos media send <chat> <file-path>
waos media download <msg-id>

waos group list
waos group info <id>
waos group members <id>

waos community list
waos community info <id>

waos status list
waos status post <text|image|video> <file>
waos status delete <id>

waos contact search <query>
waos contact info <jid>

waos dashboard          → print the current dashboard state file
waos dashboard watch    → tail the dashboard file for changes

waos sync               → trigger full resync
waos status             → connection health, uptime, queue depth
```

### Output Convention

Every CLI command outputs JSON to stdout:

```json
{
  "success": true,
  "data": { ... },
  "meta": {
    "timestamp": 1719000000,
    "duration_ms": 12
  }
}
```

Errors:

```json
{
  "success": false,
  "error": "chat not found",
  "code": "CHAT_NOT_FOUND",
  "meta": { "timestamp": 1719000000 }
}
```

### Agent Invocation Pattern

```
# Agent checks state
cat ./srv/waos/dashboard.json

# Agent reads an unread chat
waos chat open "123456789@s.whatsapp.net"

# Agent sends reply
waos message reply "123456789@s.whatsapp.net" "msg_abc123" "Yes, the quote is ready."

# Agent marks as read
waos chat mark-read "123456789@s.whatsapp.net"
```

---

## Component 8: Memory Layer

### Purpose

Each conversation has persistent, agent-maintained memory so the agent doesn't need to re-read entire message histories every time.

### Storage

Stored in SQLite `memory` table. The agent writes to it explicitly:

```
# Agent stores context after conversation
waos memory set "123456789@s.whatsapp.net" \
  '{"notes": "Interested in Rust consulting", "topics": ["rust", "drone"], "pending": "send pricing doc"}'

# Agent reads memory before replying
waos memory get "123456789@s.whatsapp.net"
→ { "notes": "Interested in Rust consulting", ... }
```

### Automatic Hints

The bridge automatically extracts:

- Topics mentioned in the last 50 messages (keyword frequency)
- People involved (from group conversations)
- Last action taken by the agent
- Follow-up deadlines (parsed from agent notes)

These are surfaced in the dashboard.

---

## Component 9: Relationship Graph

### Purpose

Represent WhatsApp as a social network so the AI can reason about context.

### Storage

In-memory graph built on startup from the contacts, groups, and communities tables + relationship scoring.

### Data

```json
{
  "nodes": [
    { "id": "alice@s.whatsapp.net", "type": "contact", "name": "Alice", "score": 0.9 },
    { "id": "bob@s.whatsapp.net", "type": "contact", "name": "Bob", "score": 0.3 },
    { "id": "dev-group@g.us", "type": "group", "name": "Dev Team" },
    { "id": "comm_abc", "type": "community", "name": "Rust Lang Community" }
  ],
  "edges": [
    { "from": "alice@s.whatsapp.net", "to": "dev-group@g.us", "label": "member" },
    { "from": "alice@s.whatsapp.net", "to": "bob@s.whatsapp.net", "label": "mutual_group", "group": "dev-group@g.us" },
    { "from": "dev-group@g.us", "to": "comm_abc", "label": "belongs_to" }
  ]
}
```

### Agent Queries

```
# Find shared groups
waos graph shared-groups "alice@s.whatsapp.net" "bob@s.whatsapp.net"

# Find introductions (A and B in same group, B joined after A)
waos graph introductions "alice@s.whatsapp.net"

# Find highest-scored contacts in a community
waos graph important-contacts "comm_abc"
```

### Scoring

Relationship scores update automatically:

- `+0.1` per direct message exchange
- `+0.05` per group interaction
- `+0.2` if contact introduced the agent to someone
- `-0.05` per week of no interaction (decay)
- Manual override via `waos contact score <jid> <value>`

---

## Component 10: Agent Dashboard

### Purpose

The agent dashboard is a **live file** (JSON on disk) that acts as the agent's "homescreen" — it represents everything the agent needs to know about its WhatsApp world at a glance.

This is NOT a human UI. It is a structured data file that the AI agent loads to understand the current state. The agent reads this file before deciding what to do.

### File Location

```
./srv/waos/dashboard.json
```

Updated in real-time on every event. The agent can `tail -f` or `inotifywait` for changes.

### Schema

Full specification is in the [Agent Dashboard](#agent-dashboard-specification) section below.

---

## Component 11: Anti-Ban Safety System

### Purpose

Keep the WhatsApp account alive by mimicking human behavior patterns.

### Rules

All configurable via environment variables or config file:

| Rule | Default | Description |
|---|---|---|
| `SEND_MIN_DELAY_MS` | 1200 | Minimum ms between outgoing messages |
| `SEND_JITTER_MS` | 600 | Random variance added to delays |
| `READ_RECEIPT_INTERVAL_S` | 5 | Batch read receipts every N seconds |
| `MAX_SENDS_PER_HOUR` | 80 | Hard cap on outbound messages per hour |
| `MAX_SENDS_PER_DAY` | 800 | Hard cap on outbound messages per day |
| `TYPING_SIM_ENABLED` | true | Show composing indicator before sending |
| `TYPING_SIM_DURATION_MS` | 1200 | How long to "type" before sending |
| `DAILY_RESET_HOUR` | 4 | Hour (local) when daily counters reset |
| `BROADCAST_CHUNK_SIZE` | 5 | Max contacts per status broadcast |

### Enforcement

- Bridge layer rejects sends that exceed limits before they reach WhatsApp
- Counters persist in SQLite across restarts
- Emergency cooldown: if 429 (rate limited) received, stop all sends for 1 hour then gradually resume
- Warning logged at 80% of daily cap

---

## Component 12: Daemon Process

### Purpose

Single long-lived binary that manages the WhatsApp connection, bridge, event bus, sync engine, and CLI tool.

### Process Architecture

```
waos daemon
├── wa-rs connections (one per account, persistent WebSocket)
├── Bridge (dedup, anti-ban, queue consumer)
├── Event Bus (tokio::sync::broadcast)
├── Synchronization Engine (event → SQLite)
├── Attention Engine (event → priority queue)
├── Dashboard Writer (event → dashboard.json)
├── Webhook Push Client (POSTs to agent gateway on high-priority events)
├── CLI dispatcher (Unix socket for local IPC)
└── Management API (optional, hyper HTTP server)
    ├── Serve frontend dist folder (QR scanning, account management, status)
    └── REST API routes (account CRUD, connection status, health)
```

### Lifecycle

```
waos daemon start      → launches background process
waos daemon stop       → graceful shutdown (flush queue, disconnect)
waos daemon status     → show connection state, uptime, queue depth
waos daemon logs       → tail logs
```

### Health Endpoint

```
GET /health → {
  "status": "connected",
  "uptime_seconds": 3600,
  "queue_depth": 0,
  "messages_sent_today": 12,
  "messages_received_today": 47,
  "chats_total": 156,
  "unread_total": 8,
  "last_event_seconds_ago": 3
}
```

---

## Component 13: Management API (optional)

### Purpose

An optional hyper-based HTTP server that serves a frontend SPA and REST API for human supervision, QR code scanning, and account management.

This is NOT the agent dashboard. The agent dashboard is a file. This is for humans who want to visually check on the system, scan QR codes in a browser, or manage accounts without editing config files.

### Design

Lightweight, single-purpose. Not a full web framework — just `hyper` + `http` + `tower` for serving static files and a thin REST API.

```
waos serve                  → starts optional hyper server on port 8080
waos serve --port 3000      → custom port
waos serve --no-frontend    → API only, no SPA serving
```

### API Routes

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/health` | Connection status, uptime, queue depth |
| `GET` | `/api/accounts` | List all configured accounts with connection state |
| `GET` | `/api/accounts/{alias}` | Single account details |
| `POST` | `/api/accounts` | Add new account config |
| `DELETE` | `/api/accounts/{alias}` | Remove account |
| `GET` | `/api/accounts/{alias}/qr` | Get QR code as base64 PNG (for pairing) |
| `GET` | `/api/dashboard` | Return current dashboard.json contents |
| `GET` | `/api/health` | Detailed daemon health metrics |

### Frontend

A compiled SPA (React or vanilla JS) placed in `./srv/waos/web/`. Hyper serves it as static files:

- Account overview cards (connection status, unread counts per account)
- QR code display for pairing new accounts
- Real-time dashboard view (polls `GET /api/dashboard` or watches delta file)
- Account add/remove/edit UI
- Health monitoring panel

### Implementation Intent

```rust
// Minimal, no Axum. Just hyper + router + static file serving.
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Request, Response, Server};

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/health") => health_handler().await,
        (&Method::GET, "/api/accounts") => accounts_list_handler().await,
        (&Method::GET, path) if path.starts_with("/api/") => api_route(path).await,
        _ => serve_static_file(path).await,  // frontend SPA fallback
    }
}
```

### Boundary

The management API is **optional**. WAOS runs headless without it. The frontend is a convenience for humans, never required for agent operation.

---

This is the core data structure the AI agent loads to understand its world.

### File Location

```
./srv/waos/dashboard.json
```

### Refresh Policy

Updated synchronously on every event. The agent can:

1. Read the file on demand
2. `inotifywait` for changes
3. `waos dashboard watch` to stream deltas

### Full Schema

```jsonc
{
  // ─── ACCOUNTS (multi-account support) ──────────
  "accounts": {
    "default": {
      "push_name": "AI Assistant",
      "phone": "15551234567",
      "connection": "connected",         // "connected" | "reconnecting" | "disconnected"
      "connected_since": 1719000000,
      "last_event_at": 1719000123,
      "unread": 3
    },
    "business": {
      "push_name": "Business Bot",
      "phone": "15559876543",
      "connection": "connected",
      "connected_since": 1718990000,
      "last_event_at": 1719000100,
      "unread": 7
    }
  },

  // ─── SUMMARY TOTALS (quick overview) ───────────
  "summary": {
    "total_accounts": 2,
    "total_connected": 2,
    "total_chats": 156,
    "total_unread": 10,
    "total_unread_direct": 3,
    "total_unread_groups": 4,
    "total_unread_communities": 1,
    "total_mentions": 2,
    "total_pending_replies": 5,
    "total_messages_today": 47,
    "total_messages_sent_today": 12,
    "total_contacts": 89,
    "total_groups": 14,
    "total_communities": 3
  },

  // ─── ATTENTION QUEUE (prioritized) ────────────
  "attention_queue": [
    {
      "rank": 1,
      "chat_id": "123456789@s.whatsapp.net",
      "chat_name": "John Doe",
      "type": "direct",
      "unread_count": 2,
      "priority_score": 0.92,
      "reason": "High importance contact, 2 unread",
      "last_message_snippet": "Hey, have you had a chance to review the proposal?",
      "last_message_at": 1719000100,
      "has_mention": false,
      "agent_memory": {
        "notes": "Interested in Rust consulting for drone project",
        "topics": ["rust", "drone", "pricing"],
        "pending": "send pricing document"
      }
    },
    {
      "rank": 2,
      "chat_id": "987654321@g.us",
      "chat_name": "Dev Team",
      "type": "group",
      "unread_count": 12,
      "priority_score": 0.88,
      "reason": "Agent mentioned by alice",
      "last_message_snippet": "@agent can you review the PR when you get a chance?",
      "last_message_at": 1719000050,
      "has_mention": true,
      "mention_count": 1,
      "agent_memory": {}
    },
    {
      "rank": 3,
      "chat_id": "555666777@s.whatsapp.net",
      "chat_name": "Sarah (Client)",
      "type": "direct",
      "unread_count": 1,
      "priority_score": 0.75,
      "reason": "Business contact, 1 unread, 2h old",
      "last_message_snippet": "Can we schedule a call next week?",
      "last_message_at": 1718990000,
      "has_mention": false,
      "agent_memory": {
        "notes": "Potential enterprise client, interested in platform demo",
        "topics": ["demo", "enterprise", "pricing"],
        "pending": "send calendar invite"
      }
    }
  ],

  // ─── ALL UNREAD CHATS (full list) ────────────
  "unread_chats": [
    {
      "chat_id": "123456789@s.whatsapp.net",
      "chat_name": "John Doe",
      "type": "direct",
      "unread_count": 2,
      "last_message_snippet": "Hey, have you had a chance to review the proposal?",
      "last_message_at": 1719000100,
      "is_muted": false,
      "contact_score": 0.9
    },
    {
      "chat_id": "987654321@g.us",
      "chat_name": "Dev Team",
      "type": "group",
      "unread_count": 12,
      "last_message_snippet": "@agent can you review the PR when you get a chance?",
      "last_message_at": 1719000050,
      "is_muted": false,
      "mention": true,
      "member_count": 8
    }
    // ... more
  ],

  // ─── PENDING REPLIES (messages agent needs to respond to) ───
  "pending_replies": [
    {
      "chat_id": "123456789@s.whatsapp.net",
      "chat_name": "John Doe",
      "type": "direct",
      "message_id": "msg_abc123",
      "message_snippet": "Hey, have you had a chance to review the proposal?",
      "received_at": 1719000100,
      "priority": "high",
      "memory_hint": "already discussed pricing range"
    }
  ],

  // ─── MENTIONS (where agent was @mentioned) ───
  "mentions": [
    {
      "chat_id": "987654321@g.us",
      "chat_name": "Dev Team",
      "mentioned_by": "alice",
      "message_snippet": "@agent can you review the PR when you get a chance?",
      "timestamp": 1719000050,
      "urgency": "medium"
    }
  ],

  // ─── GROUP OVERVIEW ───────────────────────────
  "groups": [
    {
      "chat_id": "987654321@g.us",
      "name": "Dev Team",
      "unread_count": 12,
      "member_count": 8,
      "has_mention": true,
      "last_activity": 1719000050,
      "description": "Engineering team discussion"
    },
    {
      "chat_id": "111222333@g.us",
      "name": "Family",
      "unread_count": 4,
      "member_count": 12,
      "has_mention": false,
      "last_activity": 1718980000,
      "description": null
    }
  ],

  // ─── COMMUNITY OVERVIEW ───────────────────────
  "communities": [
    {
      "id": "comm_abc",
      "name": "Rust Lang Community",
      "announcement": "Next meetup: Saturday 3 PM",
      "linked_groups": ["rust-beginners@g.us", "rust-advanced@g.us"],
      "unread_across_groups": 5,
      "last_activity": 1718970000
    }
  ],

  // ─── RECENT EVENTS (last 20, for context) ─────
  "recent_events": [
    { "type": "message", "chat": "John Doe", "summary": "Sent 'Hey, have you had a chance...'", "at": 1719000100 },
    { "type": "mention", "chat": "Dev Team", "summary": "alice @mentioned agent", "at": 1719000050 },
    { "type": "reaction", "chat": "Dev Team", "summary": "bob reacted 👍 to agent's message", "at": 1719000000 },
    { "type": "join", "chat": "Dev Team", "summary": "charlie joined the group", "at": 1718990000 }
  ],

  // ─── AGENT TASKS / FOLLOW-UPS ─────────────────
  "tasks": [
    {
      "id": "task_001",
      "description": "Send pricing document to John Doe",
      "chat_id": "123456789@s.whatsapp.net",
      "due_at": 1719086400,
      "status": "pending",
      "priority": "high",
      "created_at": 1718900000
    },
    {
      "id": "task_002",
      "description": "Schedule demo call with Sarah",
      "chat_id": "555666777@s.whatsapp.net",
      "due_at": 1719172800,
      "status": "pending",
      "priority": "medium",
      "created_at": 1718890000
    }
  ],

  // ─── WAOS INTERNAL HEALTH ─────────────────────
  "health": {
    "version": "0.1.0",

    // ── Daemon Uptime ──────────────────────────
    // These track WAOS daemon process liveness.
    // If WAOS crashes or is stopped, uptime_seconds resets to 0 on restart.
    // The agent can detect "WAOS was down" by comparing uptime_seconds
    // against the dashboard file's last modified time.
    "daemon_started_at": 1719000000,
    "uptime_seconds": 3600,
    "uptime_percent_24h": 99.8,           // tracked in SQLite across restarts
    "downtime_episodes_24h": 1,
    "last_downtime_started_at": 1718996400,
    "last_downtime_ended_at": 1719000000,
    "last_downtime_duration_seconds": 240,
    "total_downtime_seconds_24h": 43,

    // ── Network Uptime ────────────────────────
    // Internet connectivity + WhatsApp reachability.
    // WAOS pings a known host (1.1.1.1 or similar) every 30s
    // and tracks WhatsApp WebSocket connectivity.
    "network_state": "connected",          // "connected" | "disconnected" | "intermittent"
    "network_last_online_at": 1719000000,
    "network_last_offline_at": 1718999000,
    "network_offline_episodes_24h": 2,
    "network_total_offline_seconds_24h": 35,
    "network_ping_latency_ms": 12,
    "network_last_check_at": 1719000000,
    "network_checks_24h": 2880,
    "network_checks_failed_24h": 2,

    // ── Per-Account Connection ────────────────
    "accounts_total": 3,
    "accounts_connected": 2,
    "accounts_disconnected": 1,
    "accounts_connecting": 0,

    // ── Traffic ───────────────────────────────
    "messages_received_today": 1523,
    "messages_sent_today": 412,
    "messages_failed_today": 2,
    "media_downloaded_today": 87,
    "webhooks_sent_today": 203,
    "webhooks_failed": 0,

    // ── Pipeline ──────────────────────────────
    "pending_responses": 5,
    "outbound_queue_depth": 0,
    "outbound_queue_oldest_secs": null,
    "sync_status": "up_to_date",        // "up_to_date" | "syncing" | "lagging"
    "sync_lag_seconds": 0,

    // ── Anti-ban ──────────────────────────────
    "rate_limit_remaining_hourly": 68,
    "rate_limit_remaining_daily": 788,
    "rate_limit_hourly_cap": 80,
    "rate_limit_daily_cap": 800,
    "cooldown_active": false,
    "cooldown_remaining_secs": 0,

    // ── Resources ─────────────────────────────
    "memory_usage_mb": 45,
    "db_size_mb": 12,
    "dashboard_file_size_kb": 8,
    "media_cache_size_mb": 240,
    "media_cache_file_count": 1200,

    // ── Per-Account WebSocket Health ──────────
    "accounts_health": {
      "default": {
        "connection": "connected",
        "connected_since": 1719000000,
        "disconnected_count_24h": 0,
        "last_disconnected_at": null,
        "websocket_latency_ms": 45,
        "last_ping_secs_ago": 3,
        "reconnections_24h": 0
      },
      "business": {
        "connection": "connected",
        "connected_since": 1718990000,
        "disconnected_count_24h": 1,
        "last_disconnected_at": 1718995000,
        "websocket_latency_ms": 52,
        "last_ping_secs_ago": 7,
        "reconnections_24h": 1
      }
    },

    // ── Errors ────────────────────────────────
    "last_error": null,
    "last_error_at": null,
    "error_count_24h": 0,
    "warning_count_24h": 3,

    // ── System ────────────────────────────────
    "cpu_usage_percent": 2.1,
    "fd_count": 42,
    "event_bus_subscribers": 5,
    "event_bus_queue_usage_percent": 12
  }
}
```

### Delta Updates

Instead of rewriting the entire file on every event, the daemon writes atomic deltas:

```
./srv/waos/dashboard.json       → full state (rewritten every 5s)
./srv/waos/dashboard.delta      → last event change (written on every event)
```

Delta format:

```json
{
  "event": "NewMessage",
  "chat_id": "123456789@s.whatsapp.net",
  "summary_totals": { "total_unread": 9, "total_unread_direct": 4 },
  "attention_queue_rank": 1,
  "updated_fields": ["summary.total_unread", "attention_queue[0].unread_count"]
}
```

The agent subscribes to the delta file for real-time awareness without re-parsing the full state.

---

## Development Phases

### Phase 1 — Foundation

Goal: WhatsApp connection + data persistence.

- Rust project scaffold with `whatsapp-rust` dependency
- Authentication (QR + pair code)
- Bridge layer: dedup, message storage, anti-ban baseline
- SQLite schema (chats, messages, contacts)
- Event bus with broadcast channel
- Synchronization engine: event → SQLite
- Basic CLI to query chats and messages
- Agent dashboard file generation

### Phase 2 — Agent Awareness

Goal: The agent knows what's happening.

- Webhook trigger client (POST to zeroclaw gateway on high-priority events)
- Attention engine with priority scoring
- Dashboard delta file
- CLI: `dashboard`, `chat unread`, `chat open`
- Mention detection and alerting
- Group tracking

### Phase 3 — Agent Actions

Goal: The agent can act on WhatsApp.

- CLI: `message send/reply/react/edit/delete`
- CLI: `media send/download`
- Outbound queue with durability
- Rate limiting and anti-ban enforcement
- Typing simulation
- Read receipt batching

### Phase 4 — Social Intelligence

Goal: The agent understands relationships.

- Contact scoring (auto + manual)
- Relationship graph (in-memory + queryable)
- Memory layer (per-conversation notes)
- CLI: `memory`, `graph`, `contact score`
- Follow-up task tracking
- Community awareness

### Phase 5 — Autonomous Operation

Goal: The agent operates proactively.

- Goal-driven agent orchestration
- Periodic dashboard check → decide actions
- Multi-agent awareness
- Long-term memory integration (vector store)
- Performance optimization
- Production hardening

---

## Technology Stack

| Layer | Technology |
|---|---|
| WhatsApp Protocol | `whatsapp-rust` (oxidezap fork, commit `cbcdd2a`) — same pin as zeroclaw |
| Async Runtime | Tokio (multi-threaded) |
| Storage | SQLite via `rusqlite` or `sqlx` |
| Event Bus | `tokio::sync::broadcast` |
| HTTP Client (webhook push) | `reqwest` or `ureq` (WAOS is client only, no server) |
| CLI Framework | `clap` v4 |
| Media Processing | `image`, `ffmpeg` bindings |
| Serialization | `serde` + `serde_json` |
| Logging | `tracing` |
| Management API (optional) | `hyper` + `http` (serve frontend dist + REST API) |
| File Watching | `inotify` / `kqueue` for dashboard deltas |

---

## What to Reuse from Zeroclaw

Zeroclaw (zeroclaw-labs/zeroclaw) at commit `cbcdd2a` already has a working WhatsApp Web channel. Instead of building everything from scratch, reuse these proven implementations. File paths are relative to the zeroclaw repository root.

### 1. wa-rs Storage Trait Implementations

**File:** `crates/zeroclaw-channels/src/whatsapp_storage.rs`

Zeroclaw implements all four wa-rs storage traits (`SignalStore`, `AppSyncStore`, `ProtocolStore`, `DeviceStore`) using a custom `RusqliteStore` backed by `rusqlite` (not Diesel). This avoids the `libsqlite3-sys` version conflicts that come with the upstream `wa-rs-sqlite-storage`.

Copy the entire file — it handles:
- Signal protocol identity keys, sessions, prekeys, sender keys
- App state sync (versioned key-value store for chat settings)
- WhatsApp Web protocol storage (LID→PN mapping, device registry, TC tokens)
- Device identity and credentials

This is the most critical piece to reuse since it's tightly coupled to wa-rs internals.

### 2. Multi-Account Config Pattern

**Files:**
- `crates/zeroclaw-config/src/schema.rs` (search for `WhatsAppConfig` struct)
- `crates/zeroclaw-channels/src/orchestrator/mod.rs` (channel orchestration)

Zeroclaw uses `HashMap<String, WhatsAppConfig>` keyed by alias to support unlimited accounts. The orchestrator iterates all aliases and spawns a separate channel instance per alias. Reuse this pattern for WAOS account management.

### 3. Bot Lifecycle & Reconnection Logic

**File:** `crates/zeroclaw-channels/src/whatsapp_web.rs` (search for `listen` method)

Zeroclaw's `WhatsAppWebChannel::listen()` implements:
- Device loading from storage (existing session or new pairing)
- Bot construction with transport, HTTP client, runtime
- QR code rendering to stderr + pair code fallback
- Exponential backoff reconnection (10 retries, 3s–300s delays)
- Session revocation detection and cleanup
- Graceful shutdown via broadcast channel + abort handle

This is ~3500 lines of production-tested code. Reuse the patterns rather than reimplementing from scratch.

### 4. Event Handling & Message Parsing

**File:** `crates/zeroclaw-channels/src/whatsapp_web.rs` (search for `on_event` closure)

The event handler extracts:
- Sender JID and chat JID from `Event::Message`
- LID→phone number resolution via `client.get_lid_pn_entry()`
- Voice note transcription (STT) integration
- Media attachment collection (images, video, stickers, audio)
- Quoted message and media extraction
- Allowlist and group filtering

Adapt this for WAOS by writing to the replica DB instead of sending to an LLM channel.

### 5. Dependencies & Pinning

**File:** `crates/zeroclaw-channels/Cargo.toml` (lines 131–153)

Zeroclaw pins `oxidezap/whatsapp-rust` to commit `cbcdd2a6b931d804f3c2693554dce75e654834e3`:

```toml
whatsapp-rust = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
wacore = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
wacore-binary = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
waproto = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
whatsapp-rust-tokio-transport = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
whatsapp-rust-ureq-http-client = { git = "https://github.com/oxidezap/whatsapp-rust", rev = "cbcdd2a6b931d804f3c2693554dce75e654834e3" }
```

Use the same commit pin (or newer) for WAOS to ensure compatibility.

### 6. wa-rs Version Used

Zeroclaw uses `wa-rs` — the `oxidezap/whatsapp-rust` fork (NOT the original crate from crates.io). Key differences:
- Stable Rust support (no nightly required)
- Bug fixes for pairing error dispatch (`Event::PairError`)
- Active maintenance through the oxidezap org (618★, v0.6.0)

WAOS should use the same fork at the same pinned commit for consistency with zeroclaw.

---

```
/usr/local/bin/
├── waos                  # single binary (daemon + CLI)

./srv/waos/
├── whatsapp.db           # SQLite replica
├── dashboard.json        # live agent dashboard
├── dashboard.delta       # last event delta
├── media/                # downloaded media cache
│   ├── images/
│   ├── videos/
│   ├── audio/
│   └── documents/
├── sessions/             # wa-rs session data (one subdir per account)
│   ├── default/
│   ├── business/
│   └── personal/
├── web/                  # frontend dist (served by optional hyper server)
│   ├── index.html
│   ├── assets/
│   └── ...
└── waos.log

/etc/waos/
├── config.toml           # accounts, anti-ban settings, web server port
└── agents/               # agent-specific configs
    └── default.toml
```

---

## End Goal

Create a WhatsApp Agent Operating System (WAOS) where AI agents perceive WhatsApp as a living environment rather than a stream of messages.

The platform provides:

- Complete account awareness
- Event-driven perception
- Long-term memory
- Relationship understanding
- Autonomous participation
- Scalable multi-agent operation

...while remaining fully headless and controllable through structured CLI tools and a live state file that any AI agent can load and understand.
