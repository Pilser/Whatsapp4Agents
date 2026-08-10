# Per-Session DB Refactor — Final Plan & Decisions

Split the whiteboard and serverless ("apps") data out of the single shared turso DB
into **per-session database files** (`{whiteboard_dir}/sessions/{sid}/{sid}.db`),
leaving only system tables in the shared DB. This gives every session physical
isolation of its whiteboard + app data while keeping global auth, key, hook-outbox,
and routing tables in one place.

---

## 1. Why

Today one `DbPool` (one turso `Database` + one mutex-guarded `Connection`) holds every
table: system (`users`, `sessions`), whiteboard (`whiteboard_*`), and serverless
(`wb_apps`, `wb_records`, `wb_keys`, `wb_hooks`, `wb_hook_deliveries`, `wb_links`,
`wb_audit`). Every session's whiteboard + app data shares one file, so:

- No physical isolation between sessions (any owner-key bug is a data-leak vector).
- The shared DB grows unbounded with every board's records/audit.
- Migrations and schema are a single monolith.

The `docs/serverless-whiteboard-apps.md` plan already anticipated this: per-session
SQLite "mounted via ATTACH" is promoted here to a **first-class per-session DB file**.

---

## 2. Architecture

```
┌─ System DB (shared turso file, `{db_path}`) ─────────────────────────┐
│  users, sessions                               (auth/session)        │
│  wb_keys, wb_hook_deliveries                   (global, see §3)      │
│  srv_board_index(board_id, session_id, owner_key)  (routing table)   │
│  wb_public_index(slug, session_id, page_no)    (public slug index)   │
└──────────────────────────────────────────────────────────────────────┘

┌─ Per-session DB  sessions/{sid}/{sid}.db ────────────────────────────┐
│  whiteboard_pages, whiteboard_meta, whiteboard_public_pages          │
│  wb_apps, wb_records, wb_hooks, wb_links, wb_audit                   │
└──────────────────────────────────────────────────────────────────────┘
```

- A single shared `DbPool` remains for the **system DB** only.
- Each session's DB file is opened lazily and cached in a
  `HashMap<String, Arc<DbPool>>` guarded by a mutex (`SessionStore`).
- `turso::Builder::new_local(path)` per path = independent DB file, fully supported.
- Session files already live at `sessions/{sid}/`; the DB joins them as `{sid}.db`.

---

## 3. Decisions (locked)

| # | Decision |
|---|---|
| 1 | **`wb_keys` stays a global table in the system DB.** Key rows (`bucket, board_id, key_hash, role, writer`) must survive DB-file churn and be queryable across sessions; the `key_hash` lookup is global auth. |
| 2 | **`wb_hook_deliveries` stays a global table in the system DB** — it is a cross-session **outbox** (delivery rows for any board, drained by a global worker). |
| 3 | **Web boards are session-owned**, exactly like MCP boards. Every board (web or MCP) is created by a session and routed via `srv_board_index`. |
| 4 | **Data split is done by the user via the turso CLI**, not by our code. We ship the schema + `SessionStore`; the actual `{sid}.db` files are populated with existing data by hand/CLI. No automatic in-repo data migration code. |
| 5 | **Keep vendored turso `0.7.0-pre.10` as-is** (`zerowrapper/Cargo.toml:34` → `../turso/bindings/rust`). No upstream upgrade. |
| 6 | `wb_hooks` (board → URL registry) is a **session-DB table**; the routing decision lives in `SessionStore.hook_urls`. |
| 7 | `wb_links` (board linking) is a **session-DB table**. |
| 8 | `wb_audit` is a **session-DB table** (per-board audit, local to the session). |
| 9 | Ownership-check methods in `keys.rs` (`srv_key_issue/list/revoke`) and `hook_deliveries.rs` (`srv_hook_reschedule/deliveries`) query `wb_apps` for ownership; since `wb_apps` moves to session DBs, **those checks are removed** (signature keeps `_owner` param) and **enforced by `SessionStore`** which routes + checks against the session DB. |
| 10 | `srv_key_auth`, `srv_board_has_keys`, `srv_hook_enqueue`, `srv_hook_due`, `srv_hook_mark` stay in `zw-db` on the system pool (pure global queries; no `wb_apps` dependency). |

---

## 4. New / changed tables

**System DB** (keeps existing `users`, `sessions`, `wb_keys`, `wb_hook_deliveries`):

```sql
CREATE TABLE IF NOT EXISTS srv_board_index (
    board_id   TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    owner_key  TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX IF NOT EXISTS idx_srv_board_index_session ON srv_board_index(session_id);

CREATE TABLE IF NOT EXISTS wb_public_index (
    slug       TEXT PRIMARY KEY,
    session_id TEXT NOT NULL,
    page_no    INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

**Per-session DB** (`sessions/{sid}/{sid}.db`): `whiteboard_pages`,
`whiteboard_meta`, `whiteboard_public_pages`, `wb_apps`, `wb_records`, `wb_hooks`,
`wb_links`, `wb_audit` — the existing `whiteboard`/`serverless` schemas verbatim.

---

## 5. `SessionStore` (new module `zw-db/src/session_store.rs`)

```rust
pub struct SessionStore {
    system: DbPool,                        // shared system pool (auth, keys, outbox, index)
    root: Arc<PathBuf>,                    // whiteboard_dir (sessions live under root/sessions)
    open: Arc<Mutex<HashMap<String, Arc<DbPool>>>>,  // sid -> lazy session pool
}
```

**Routing:**
- `session(sid)` — lazily open + cache `sessions/{sid}/{sid}.db`, `migrate_session()`.
- `board(board_id)` — system-index lookup → `session(session_id)`.
- `board_pool(board_id)` — returns the session `DbPool` for a board.
- `session_id_for_board(board_id)` — system-index only (no DB open).
- `register_board(sid, board_id, owner_key)` / `unregister_board(board_id)` — maintain
  `srv_board_index` (called on create/delete board).
- `all_session_ids()` — scan `sessions/*/` dirs (used by TTL sweeps).

**Boundary operations** (system ↔ session, where a single op must touch both):
- `create_board`, `delete_board`
- `key_issue` / `key_list` / `key_revoke` (ownership check vs session DB, rows in system DB)
- `set_page_public` / `public_snapshot` (content in session DB, slug index in system)
- `hook_urls` (routing) / `hook_enqueue` / `hook_reschedule` / `hook_deliveries`
- `ttl_sweep_all` (scan all session DB files)

**Concurrency rule:** never hold the system conn guard across an `.await` of another
DB op — scope the query, drop the guard, then open the session pool.

---

## 6. Ownership enforcement

`wb_apps` (the owner table) now lives in the session DB. Any op that needs to prove
"board X is owned by session S" must open the session DB and check `wb_apps.owner_key`
there, then act on the system DB (keys, outbox, index). The `_owner` params stay in the
`zw-db` method signatures so call sites keep working; the actual check happens in
`SessionStore` before delegating to the underlying pool method.

---

## 7. Implementation phases

- **Phase A — `zw-db` foundation:** split `migrations.rs` into `SYSTEM_SCHEMA` /
  `SESSION_SCHEMA` / `SYSTEM_MIGRATIONS` / `SESSION_MIGRATIONS`; add
  `DbPool::migrate_session()`; new `SessionStore`; drop `wb_apps` ownership checks in
  `keys.rs` + `hook_deliveries.rs`. Verify: `cargo check`.
- **Phase B — `zw-whiteboard`:** swap `db: Option<DbPool>` → `Option<SessionStore>`
  (`with_db`, `db()`, and every internal caller).
- **Phase C — `zw-mcp`:** `srv.rs` USAGE/execute + `manage_whiteboard.rs` use
  `SessionStore` (routing by board → session).
- **Phase D — `ch-web-ui` + `main.rs`:** build `SessionStore` from whiteboard_dir,
  pass into `AppContext`, route serverless/public_wb/ws handlers and webhook/ttl
  workers through it.

Each phase: one concern per file, explicit imports, `cargo check` only.

---

## 8. Verification rules

- **Only `cargo check`** for verifying compilation during development.
- Never `cargo build` / `cargo build --release` / `cargo clean`; never touch `target/`.
- Existing `web/` frontend builds with `bun run build`.
- Sub-agents follow the same rules.

---

## 9. Backward compatibility

- Whiteboard loader resolution already supports `sessions/{sid}/{sid}.wb`; DB access is
  the only change here.
- System-DB-only call sites (`upsert_session`, `users`, `sessions`) are untouched.
- Data split (`{sid}.db` population) is done by the user via the turso CLI (decision #4).
