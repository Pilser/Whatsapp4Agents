# Multi-Page Whiteboard (Design)

A session's whiteboard becomes a **deck of pages**. The database is the single
source of truth for pages, their order, their state (open/locked), and public
shareability. The agent drives page management through **one MCP tool shaped like
a CLI** (`manage_whiteboard`), and writes page content with its existing file tool.

## Status

Discussion phase. Design below is the agreed direction; the CLI-over-MCP surface
is still under discussion.

---

## Current behavior (single page, file-based)

1. Agent writes/updates `<session_id>.wb` markdown in its workspace (its own file tools).
2. `zw-whiteboard` monitor polls files every 500ms (`zw-whiteboard/src/lib.rs`),
   detects change via mtime/size (`monitor.rs`).
3. On change: parse markdown -> resolve pins -> `cache.resolve()` -> `/files/<hash>`
   URLs -> `ParsedBoard { body_md, pins }`.
4. Broadcast `WhiteboardUpdate` -> WS handler (`ws/handler.rs`) -> `whiteboard_update`
   -> frontend `renderBoard`.
5. On connect/sync: frontend sends `whiteboard_sync {session_id}`; server `ensure()`s
   the file and returns the current board.

`zw-db` is Turso (local libsql file, shared `zerowrapper.db`) with `users`/`sessions`.
Whiteboard pages slot in next to those tables.

---

## DB model

```sql
CREATE TABLE whiteboard_pages (
  session_id TEXT NOT NULL,
  page_no    INTEGER NOT NULL,
  title      TEXT NOT NULL DEFAULT '',
  body_md    TEXT NOT NULL DEFAULT '',
  public     INTEGER NOT NULL DEFAULT 0,     -- intent flag: shareable
  updated_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (session_id, page_no)
);

CREATE TABLE whiteboard_meta (
  session_id  TEXT PRIMARY KEY,
  active_page INTEGER,                        -- page open in agent workspace
  locked      INTEGER NOT NULL DEFAULT 0,     -- 1 = agent working on active_page
  rev         INTEGER NOT NULL DEFAULT 0,     -- bumped in every write txn
  updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE whiteboard_public_pages (
  id         TEXT PRIMARY KEY,               -- random slug, e.g. "pQ7k3m"
  session_id TEXT NOT NULL,
  page_no    INTEGER NOT NULL,               -- origin, informational only
  body_md    TEXT NOT NULL,                  -- frozen snapshot at share-time
  pins       TEXT NOT NULL,                  -- resolved pins JSON, frozen
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### Page identity & numbering
- A page is uniquely `(session_id, page_no)` via the primary key. `page_no` IS the
  number; nothing else to drift.
- Add = next free `page_no`. Delete/move = renumber inside one transaction, so there
  are never gaps or duplicates.

### Agent open page & lock
- `meta.active_page` = the page the agent currently has open in its workspace.
- `meta.locked = 1` while the agent is working on it. Set atomically by `open`.
- **Lock scope**: while locked, the page is fully read-only for the user — no
  edits, moves, or deletes. Enforced in the UI **and** server-side (the web server
  refuses writes/moves/deletes targeting `active_page` while `locked`).
- Other pages stay editable.

### Public pages (frozen snapshot)
- `whiteboard_pages.public` is the intent flag; toggled by user or agent.
- The moment `set_public` turns on, a **snapshot** is taken into
  `whiteboard_public_pages` with a random slug: current body + resolved pins.
- Share URL = `https://site/w/<slug>` (no auth). It is **immutable** — later edits
  to the page never change it. Re-sharing later (off -> on) takes a fresh snapshot.
- Lifecycle:
  - `set_public off` -> delete the snapshot (URL 404s).
  - Source page deleted -> delete its snapshot too.
  - Page moved/renumbered -> snapshot unaffected (slug-keyed, not page_no-keyed).

---

## Agent <-> file bridge

The agent already has a tool for writing content, so `manage_whiteboard` has **no
body field**. The bridge keeps the existing `.wb` file as the agent's working file
and the DB as the truth:

- `open <page_no>`: swap the workspace file `<session_id>.wb` to that page's body
  (**DB -> file**), set `active_page` + `locked`. The agent sees current content
  and edits it with its normal file tool.
- Agent edits the file.
- The existing 500ms file monitor syncs the file back (**file -> DB**) into
  `active_page`, bumping `rev` in the same transaction.
- `close`: clear the lock (file can keep the last body; it is only a mirror).

User edits from the frontend (pages not locked) write the DB directly and bump `rev`.

---

## Push path (DB -> frontend)

- Monitor polls `whiteboard_meta.rev` for known sessions every 500ms.
- On change: re-read pages + meta, resolve per-page pins, broadcast
  `whiteboard_update { session_id, pages: [{page_no, title, body_md, pins, public}], active_page, locked }`.
- Frontend navigator renders "Page X of N", shows the lock badge on the active page,
  and a share button (copies `/w/<slug>`).
- `whiteboard_sync`: returns current pages + meta; seeds page 1 (`# Whiteboard`)
  if the session is new.

---

## MCP surface: one tool, CLI-over-MCP

Exactly **one tool** — `manage_whiteboard` — that behaves like a shell CLI spoken
over MCP. This is the active discussion topic (see "CLI over MCP" below).

Proposed shape (subject to discussion):

```
manage_whiteboard { "command": "wb --session <session_id> <action> ..." }
```

`--session` is required on every command. Subcommands (all structural/state only —
no body content):

```
wb list                                 # all pages + state
wb open <page_no>                       # set active_page + lock, swap file
wb close                                # unlock
wb add [--title <t>]                    # new page at next page_no
wb delete <page_no>                     # delete + renumber (blocked if locked)
wb move <page_no> to <page_no>          # reorder (blocked if either locked)
wb set-public <page_no> on|off          # toggle intent + snapshot
wb show <page_no>                       # view one page
wb --help | wb <action> --help          # inline help for the agent
```

Resolved decisions are in the section below.

---

## CLI over MCP (decisions)

One registered tool = smaller tool list for the agent; the MCP registry stays
stable while commands grow. Subcommands + flags are easy for an LLM to emit, easy
to read in tool calls, and extensible without protocol churn. The return value
looks like a terminal: human text plus a structured result (`status`, `pages`).

Resolved decisions:

1. **Session context — required on every call.** `--session <session_id>` is
   mandatory on every command. No persistent default, no `wb use`. The agent
   always states its session explicitly, so a call can never act on the wrong
   board.
2. **Parser — hand-rolled, no dependency.** A small grammar in `zw-mcp`:
   subcommand + positional args + `--flag value`. No external CLI-parsing crate.
3. **Surface stays generic/extensible.** Other grammars will be needed later too,
   so the design stays command-based and open. Future `wb` verbs (export,
   duplicate, pin, archive) are added as subcommands without touching MCP
   registration.
4. **Help must be as helpful as possible.** Every level answers:
   - `wb --help` — full command list, one line per subcommand, quick usage example.
   - `wb <action> --help` — exact arguments, every flag with meaning + default,
     1-2 concrete examples, and what the command returns.
   - Bad/incomplete invocations print the closest help automatically.
5. **Errors must guide the agent like `--help`.** An error is never a bare
   failure. Every error response includes:
   - `status: error`
   - a clear human message stating exactly what was wrong,
   - the offending input echoed back,
   - a `hint` with a corrected example, and
   - a short `usage` snippet for the failing command.
   Blocked operations (e.g. delete/move on a locked page) explain *why* it is
   blocked and what state must change first.

---

## Implementation API contract

Freeze for parallel/stepwise agents. Types and DB access live in `zw-db`; the store +
file bridge + push live in `zw-whiteboard`; consumers are `zw-mcp`, `ch_web_ui`.

### zw-db (new `src/whiteboard.rs`, registered in `lib.rs`)

```rust
pub struct WbPage      { pub page_no: i64, pub title: String, pub body_md: String, pub public: bool }
pub struct WbMeta      { pub active_page: Option<i64>, pub locked: bool, pub rev: i64 }
pub struct PublicSnap  { pub id: String, pub session_id: String, pub page_no: i64, pub body_md: String, pub pins: String }
```
`impl DbPool` async methods (each write bumps `rev`, runs in one txn via execute_batch):
`wb_ensure_session(sid)`, `wb_state(sid) -> (Vec<WbPage>, WbMeta)`, `wb_write_page(sid, page_no, title, body_md)`,
`wb_add_page(sid,title)->i64`, `wb_delete_page(sid,page_no)` (+renumber), `wb_move_page(sid,from,to)`,
`wb_open(sid,page_no)`, `wb_close(sid)`, `wb_set_public(sid,page_no,bool)->Option<String>(slug)`,
`wb_set_public_content(slug, body_md, pins)` (fills a snapshot's frozen body+pins after resolution),
`wb_public_snapshot(slug)->Option<PublicSnap>`, `wb_delete_public_by_page(sid,page_no)`.

### zw-whiteboard (page-aware store + bridge + push)

- `struct BoardPage { pub page_no: i64, pub title: String, pub body_md: String, pub public: bool, pub pins: Vec<Pin> }`
- `struct BoardState { pub session_id: String, pub pages: Vec<BoardPage>, pub active_page: Option<i64>, pub locked: bool }`
- `WhiteboardEvent::BoardUpdate { session_id, state: BoardState }`.
- `WhiteboardHandle` keeps constructor `new(dir, cache, poll_interval)` plus `with_db(db: DbPool) -> Self`
  (db stored as `Option<DbPool>` so `main.rs` still compiles until wired).
- Store methods used by the MCP tool: `list(sid)->BoardState`, `open(sid,page_no)`,
  `close(sid)`, `add(sid,title)->i64`, `delete(sid,page_no)`, `move_pages(sid,from,to)`,
  `set_public(sid,page_no,bool)->Option<String>`. Each mirrors the `wb_*` DB call and
  handles the file side (see bridge). `set_public` resolves the page via parser+resolve
  and stores body+pins via `wb_set_public_content`.
- File bridge: the workspace file `<sid>.wb` always mirrors the **active page** body
  (raw markdown, `@pin` lines kept). `ensure`/`open` write DB body -> file; the monitor
  syncs a changed file back into the active page DB row. `close` leaves the file.
- Monitor spawn: polls DB `rev` (and file mtime for the active page) every 500ms; on
  change builds `BoardState` (per-page pins resolved via `resolve_board`) and broadcasts.
- `ensure(sid)`: seeds DB session (page 1), writes file from page 1 body if missing.
- `latest(sid) -> Option<BoardState>`: build state from DB.

### JSON over WS (`whiteboard_update`)

```json
{ "type": "whiteboard_update", "session_id": "...",
  "state": { "session_id":"...", "pages":[{"page_no":1,"title":"...","body_md":"...","public":false,"pins":[]}],
             "active_page":null, "locked":false } }
```

Frontend `setBoard` renders `state.pages[active_index]` as body + pins; shows
"Page X of N", lock badge, share button (copies `/w/<slug>` for public pages).

### MCP `manage_whiteboard`

```json
{ "command": "wb --session <sid> <action> ..." }
```
Subcommands: `list | open <no> | close | add [--title t] | delete <no> | move <from> to <to> |
set-public <no> on|off | show <no> | --help`. `--session` required on every command.
Output: `{ "status":"ok"|"error", "pages":[WbPage...], "active_page", "locked", "hint"? , "usage"? }`.

---

## What changes where

- New: `zw-db` migrations (tables above); `zw-whiteboard/src/pages.rs` (DB read/write,
  rev poll, file bridge); MCP provider `manage_whiteboard` (CLI parser + dispatch);
  public route `/w/<slug>`; page navigator + lock badge + share button in
  `whiteboard_panel.rs` and `whiteboard.js`/`whiteboard-render.js`.
- Retired: file-based `read_board_from`/`resolve_board` for board content (pin
  resolution moves to per-page, and public snapshots).
