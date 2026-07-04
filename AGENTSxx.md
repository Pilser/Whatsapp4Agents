# WAOS — AI Agent Guide

> **Canonical entry point:** [`phase-docs/index.md`](./phase-docs/index.md) — contains all operating instructions, phase map, and key file references. This file is a condensed backup.

## Error Checking

Only `cargo check` — never `cargo build` or `cargo test` unless explicitly asked.

```bash
cd waos && cargo check
```

## Phase 1 — Completed [`phase-docs/phase-1-foundation.md`](./phase-docs/phase-1-foundation.md)

- [x] **Cargo workspace** — 4 crates: `core`, `db`, `lib`, `bin`
- [x] **Config loading** — loads from `./config.toml` → `./.env` → `$XDG_CONFIG_HOME/waos/config.toml`; supports `[paths]`, `[server]`, `[[accounts]]`, `[agent]` sections
- [x] **Main DB** — `waos.db`: accounts, rate counters, tasks, health log
- [x] **Per-account DB** — `accounts/{alias}.db`: chats, messages, contacts, graph, memory, queue
- [x] **wa-rs storage traits** — `TursoStore` implements SignalStore, AppSyncStore, ProtocolStore, DeviceStore
- [x] **Event bus** — `tokio::sync::broadcast` with `WaosEvent` enum
- [x] **Sync engine** — Subscribes to event bus, writes NewMessage to per-account DB
- [x] **Dashboard writer** — Aggregates per-account stats → `dashboard.json`
- [x] **Bridge layer** — Dedup, AntiBan, OutboundQueue (in-memory)
- [x] **Bot lifecycle** — `AccountManager` spawns wa-rs Bot per account with reconnection loop (exponential backoff, max 10 retries)
- [x] **Event wiring** — wa-rs `on_event` handler publishes Connected/Disconnected/NewMessage/LoggedOut/PairingCode/QrCode to EventBus
- [x] **Daemon** — Initializes all subsystems, Ctrl+C graceful shutdown. `waos` (no args) = `waos daemon`.
- [x] **CLI** — `waos`, `waos status`, `waos pair`, `waos dashboard`, `waos chats`

## Phase 2 — Completed [`phase-docs/phase-2-agent-awareness.md`](./phase-docs/phase-2-agent-awareness.md)

- [x] **Mention detection** — `extract_mentions()` in `lib/src/bridge/mention_detector.rs`, wired into `account_manager.rs`
- [x] **Attention engine** — `AttentionEngine` in `lib/src/attention.rs` scores/ranks unread chats by recency, importance, unread count
- [x] **Dashboard delta** — `write_delta()` in `lib/src/dashboard.rs` writes `dashboard.delta` on every event
- [x] **Webhook push** — `WebhookPush` in `lib/src/webhook.rs` POSTs NewMessage/MentionReceived to agent gateway
- [x] **Config** — `AgentConfig` with `webhook_url`/`webhook_token` in `lib/src/config.rs`
- [x] **Daemon wiring** — attention engine, webhook push, dashboard delta wired in `bin/src/daemon.rs`
- [x] **CLI restructure** — `chat` subcommand group (`list`, `unread`, `search`), `dashboard show`/`watch`
- [x] **Chat CLI** — `bin/src/cmds/chat.rs`: `waos chat --account X list`, `unread`, `search`
- [x] **Dashboard watch** — `bin/src/cmds/dashboard_watch.rs`: polls and streams `dashboard.delta`
- [x] **Verification** — `cargo check` passes with 0 errors

## Phase 3 — Completed [`phase-docs/phase-3-agent-actions.md`](./phase-docs/phase-3-agent-actions.md)

- [x] **Anti-ban enforcement** — `lib/src/bridge/anti_ban.rs`: hourly/daily caps, send pacing with jitter, emergency cooldown on 429
- [x] **Outbound queue consumer** — `lib/src/bridge/queue.rs`: processes pending sends from per-account DB
- [x] **Typing simulation** — `lib/src/bridge/typing_sim.rs`: shows composing indicator before sends
- [x] **Read receipt batching** — `lib/src/bridge/read_receipt.rs`: batched mark-read every 5s
- [x] **Message sender** — `lib/src/adapter/sender.rs`: build wa-rs protobuf messages and send via Client
- [x] **Media handling** — `lib/src/media.rs`: download incoming media, upload and send outgoing
- [x] **CLI: message commands** — `bin/src/cmds/message.rs`: send, reply, react, edit, delete
- [x] **CLI: media commands** — `bin/src/cmds/media.rs`: send, download
- [x] **CLI: chat mark-read** — `bin/src/cmds/chat.rs`: add mark-read subcommand

## Phase 4+5 — Combined (Next) [`phase-docs/phase-4-social-intelligence.md`](./phase-docs/phase-4-social-intelligence.md) · [`phase-docs/phase-5-autonomous-operation.md`](./phase-docs/phase-5-autonomous-operation.md)

Phases 4 and 5 are implemented together as a unified business knowledge graph system. Key architectural changes from original spec:
- **No recursive CTEs** — Turso rejects `WITH RECURSIVE` at planner.rs:751; use application-level BFS in Rust
- **Graph edges populated from sync engine** — every NewMessage creates `interacted_with` / `talked_about` edges
- **Topic tracking via graph nodes** — topics extracted from messages become graph nodes connected to contacts
- **Vector memory (Phase 5) for semantic search** — enables "which users asked about product X?" queries
- **Cross-account merge** — normalizes contacts by phone across all per-account DBs

- [ ] **Contact scoring** — `lib/src/scoring.rs`: multi-dimensional scoring (frequency, recency, importance, reply rate, centrality)
- [ ] **Memory layer** — `lib/src/memory.rs`: per-conversation memory with topic arrays stored as JSON
- [ ] **Relationship graph** — `lib/src/graph.rs`: application-level BFS, shared-groups, introductions, cross-account merge
- [ ] **Graph edge population** — `lib/src/sync_engine.rs`: populate `graph_edges` and `contact_scores` on every NewMessage
- [ ] **Vector memory** — `lib/src/vector_memory.rs`: embed messages, semantic search via `vector_distance_cos()`, find contacts by topic
- [ ] **Embedding provider** — HTTP client to OpenAI-compatible API (OpenAI/OpenRouter/custom)
- [ ] **Orchestrator** — `lib/src/orchestration.rs`: periodic dashboard checks, proactive agent triggers, task reminders
- [ ] **CLI: contact** — `bin/src/cmds/contact.rs`: search, info, score commands
- [ ] **CLI: memory** — `bin/src/cmds/memory.rs`: get/set per-conversation memory, semantic search
- [ ] **CLI: graph** — `bin/src/cmds/graph.rs`: shared-groups, introductions, important-contacts
- [ ] **CLI: community** — `bin/src/cmds/community.rs`: list/info community commands
- [ ] **CLI: task** — `bin/src/cmds/task.rs`: list/complete agent follow-ups from `agent_tasks` table
- [ ] **Production hardening** — `bin/src/daemon.rs`: SIGTERM handling, crash recovery, resource limits
- [ ] **Health monitoring** — comprehensive health metrics in dashboard

## Phase 6 — Management UI [`phase-docs/phase-6-management-ui.md`](./phase-docs/phase-6-management-ui.md)

- [ ] **Hyper HTTP server** — REST API for accounts, health, dashboard, QR codes
- [ ] **REST API routes** — `/api/accounts`, `/api/health`, `/api/dashboard`, `/api/chats`
- [ ] **QR code endpoint** — returns base64 PNG for browser pairing
- [ ] **Static file serving** — SPA frontend served via hyper
- [ ] **Frontend SPA** — account cards, health panel, QR display, dashboard view
- [ ] **Resource monitor** — memory, disk, media cache tracking
- [ ] **Media cache cleanup** — auto-cleanup of old cached media
- [x] **Server config** — `[server]` section in `config.toml`
- [ ] **`waos serve`** CLI command

## Turso (not SQLite)

WAOS uses **Turso** (`turso/` folder, a vendored libsql fork) instead of rusqlite for all WAOS-owned replica data.

**Key differences from SQLite:**
- Async API (`conn.execute(...).await`)
- Path-based builder: `Builder::new_local(path).build().await` → `db.connect()`
- `Value` enum (not rusqlite types): `Value::Text`, `Value::Integer`, `Value::Blob`, `Value::Null`
- Positional params: `Params::Positional(vec![Value::from(x), ...])`
- Query returns rows via `stmt.query_row()` / `stmt.query()` + `rows.next().await`
- Get values: `row.get_value(index)?` returns `turso::Value`

**Example patterns** — see `waos/db/src/manager.rs`, `waos/db/src/account_schema.rs`, `waos/lib/src/sync_engine.rs`
**Turso vendor source** — `turso/core/`, `turso/bindings/rust/`

**Critical limitation:** Recursive CTEs (`WITH RECURSIVE`) are parsed but rejected at planning time (`turso/core/translate/planner.rs:751`: `"Recursive CTEs are not yet supported"`). Use application-level BFS or multi-hop SQL joins for graph traversal.

## wa-rs Wiring

The wa-rs Bot is built with this pattern (see `waos/lib/src/adapter/account_manager.rs`):

```rust
use whatsapp_rust::bot::Bot;
use whatsapp_rust::TokioRuntime;
use whatsapp_rust_tokio_transport::TokioWebSocketTransportFactory;
use whatsapp_rust_ureq_http_client::UreqHttpClient;
use wacore::store::DevicePropsOverride;
use waproto::whatsapp::device_props::PlatformType;

let mut builder = Bot::builder()
    .with_backend(backend)              // Arc<TursoStore> implements all 4 traits
    .with_transport_factory(factory)    // TokioWebSocketTransportFactory
    .with_http_client(client)           // UreqHttpClient
    .with_runtime(TokioRuntime)
    .with_device_props(DevicePropsOverride::new().with_os("WAOS").with_platform_type(PlatformType::Desktop))
    .on_event(|event, client| async move { ... });

let mut bot = builder.build().await?;
let handle = bot.run().await?;
```

**Storage traits** implemented by `TursoStore`:
- `SignalStore` — identities, sessions, prekeys, sender keys
- `AppSyncStore` — app state sync keys and versions
- `ProtocolStore` — sender key devices, LID→PN mapping, device registry, TC tokens, sent messages
- `DeviceStore` — device identity/key persistence

**Reference sources:**
- Full Bot lifecycle: `zeroclaw/crates/zeroclaw-channels/src/whatsapp_web.rs` (especially `listen()` at line ~1699)
- Storage traits adapted from: `zeroclaw/crates/zeroclaw-channels/src/whatsapp_storage.rs`
- wa-rs source: `whatsapp-rust/` (oxidezap fork, pinned at commit `4def659`)

## Hyper HTTP Server Pattern

When adding the optional management API, use **hyper** directly — no Axum. Reference pattern in `EmbeddingServer/src/server/hyper_server.rs`:

```rust
use hyper::{Body, Request, Response, Server, Method, StatusCode};
use hyper::service::{make_service_fn, service_fn};

let make_svc = make_service_fn(move |_| {
    let state = state.clone();
    async move {
        Ok::<_, Infallible>(service_fn(move |req| {
            let state = state.clone();
            handle_request(req, state)
        }))
    }
});

let server = Server::from_tcp(listener)?
    .http1_keepalive(true)
    .serve(make_svc);
server.await?;
```

Routing is done via manual match on `(method, path)` — no router crate needed.

## Account Management

Accounts are **managed via the main DB `accounts` table**, NOT from config files.

**Flow:**
1. On daemon start, `config.resolve_accounts()` queries the main DB `accounts` table
2. If the DB is empty (first run), config `[[accounts]]` is bootstrapped into the DB
3. After that, all account CRUD goes through the **REST API** (Phase 6)
4. The `DaemonState` struct in `bin/src/daemon.rs` provides `init_account()` / `destroy_account()`
5. `destroy_account()` fully cleans up: stops Bot, deletes per-account DB, wipes session credentials, removes from main DB

**Config `[[accounts]]`** is bootstrap-only. To manage accounts after first run:
```bash
# Add account (when Phase 6 server is running)
curl -X POST http://localhost:8080/api/accounts \
  -H "Content-Type: application/json" \
  -d '{"alias":"my_bot","phone":"+1234567890"}'

# Delete account (removes everything including WhatsApp credentials)
curl -X DELETE http://localhost:8080/api/accounts/my_bot
```

**Database tables involved:**
- `main_db.accounts` — account registry (alias, phone, status)
- `accounts/{alias}.db` — per-account chats, messages, graph, memory
- `sessions/{alias}/` — wa-rs credential files (deleted on account removal)

## Key Files

| Area | File |
|---|---|
| Workspace Cargo.toml | `waos/Cargo.toml` |
| Event types | `waos/core/src/events.rs` |
| Data types | `waos/core/src/types.rs` |
| Main DB schema | `waos/db/src/main_schema.rs` |
| Account DB schema | `waos/db/src/account_schema.rs` |
| DB manager | `waos/db/src/manager.rs` |
| Config | `waos/lib/src/config.rs` |
| Config file (TOML) | `waos/config.toml` |
| Config file (.env) | `waos/.env` |
| CLI + daemon | `waos/bin/src/` |
| Daemon state + account lifecycle | `waos/bin/src/daemon.rs` (`DaemonState`, `init_account`, `destroy_account`) |
| Event bus | `waos/lib/src/event_bus.rs` |
| Bot lifecycle | `waos/lib/src/adapter/account_manager.rs` |
| wa-rs storage traits | `waos/lib/src/adapter/storage/` |
| Sync engine | `waos/lib/src/sync_engine.rs` |
| Dashboard writer | `waos/lib/src/dashboard.rs` |
| Bridge: dedup | `waos/lib/src/bridge/dedup.rs` |
| Bridge: anti-ban | `waos/lib/src/bridge/anti_ban.rs` |
| Bridge: queue | `waos/lib/src/bridge/queue.rs` |
| Bridge: mention detector | `waos/lib/src/bridge/mention_detector.rs` |
| Attention engine | `waos/lib/src/attention.rs` |
| Webhook push | `waos/lib/src/webhook.rs` |
| Contact scoring | `waos/lib/src/scoring.rs` |
| Memory layer | `waos/lib/src/memory.rs` |
| Relationship graph | `waos/lib/src/graph.rs` |
| Vector memory | `waos/lib/src/vector_memory.rs` |
| Orchestrator | `waos/lib/src/orchestration.rs` |
| Chat CLI | `waos/bin/src/cmds/chat.rs` |
| Message CLI | `waos/bin/src/cmds/message.rs` |
| Media CLI | `waos/bin/src/cmds/media.rs` |
| Contact CLI | `waos/bin/src/cmds/contact.rs` |
| Memory CLI | `waos/bin/src/cmds/memory.rs` |
| Graph CLI | `waos/bin/src/cmds/graph.rs` |
| Community CLI | `waos/bin/src/cmds/community.rs` |
| Task CLI | `waos/bin/src/cmds/task.rs` |
| Dashboard watch CLI | `waos/bin/src/cmds/dashboard_watch.rs` |
| CLI + daemon | `waos/bin/src/` |
| Zeroclaw reference | `zeroclaw/crates/zeroclaw-channels/src/whatsapp_web.rs` |
| wa-rs source | `whatsapp-rust/` |
| Hyper reference | `EmbeddingServer/src/server/hyper_server.rs` |
| Architecture plan | `WAOS Plan.md` |



https://www.youtube.com/watch?v=H7HmzwI67ec&list=RDH7HmzwI67ec


vestige dashboard      # → http://localhost:3927/dashboard
