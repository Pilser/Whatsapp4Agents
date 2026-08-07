# Serverless "Apps" Inside Whiteboards — Final Plan

Serverless-style apps built purely with the HTML/JS already supported by whiteboards.
Each whiteboard can become a tiny app that **writes, reads, and shows its own data**,
with **files stored in the owning session's folder**, all managed by the agent through
new sub-commands on the existing `manage_whiteboard` MCP tool. No per-app backend
code, no new database server — the same SQLite/libSQL already in the stack.

---

## 1. Concept

A whiteboard embed runs arbitrary HTML/JS (already executed via `runScripts=true`).
That JS calls **generic routes** handled by **one catch-all handler** — a single piece
of code with no per-app logic. Because routes are just lookups against a registry
table (`wb_apps`), a new "app" is created by **one database row**, not by writing code.

```
┌─ Whiteboard app (HTML+JS only) ──────────────────────────┐
│  fetch("/api/srv/<board_id>/submit", {body})   → write    │
│  fetch("/api/srv/<board_id>/records")          → read     │
│  form.append("file", …)                        → upload   │
└──────────────────────┬───────────────────────────────────┘
                       ▼
     Catch-all handler — ONE generic function
      1. board_id in wb_apps ?      →  404 if unknown
      2. validate payload vs schema →  400 if invalid
      3. INSERT into wb_records     →  201
                       ▼
   SQLite/libSQL (metadata, queryable, owner-scoped)
   +  session folder (file blobs, per-session locality)
```

**"One endpoint" clarified:** I meant one *handler* (one generic piece of code), not a
single literal URL. That handler serves a small **family of generic routes** — so a
board can not only add content but also **list/get its own content and render it back
in the whiteboard** (feedback wall, guestbook, gallery, form results, …). The table of
routes is in §4.

---

## 2. Session folder model (everything about a session in one place)

Today `.wb` files are flat: `<workspace>/{sid}.wb`. We move to a per-session tree so a
session's *everything* lives together, including app files. Legacy flat files are
**migrated once** (renamed) into the session folder on first access — there is no
fallback path anymore:

```
<workspace>/sessions/{sid}/
├── {sid}.wb                          # the whiteboard itself
└── apps/
    └── {board_id}/
        ├── files/                    # uploaded blobs: {file_id}.bin
        └── app.json                  # optional per-board notes/metadata
```

- **Whiteboard file** relocates to `sessions/{sid}/{sid}.wb` (read-back stays
  compatible: lookup by session in the folder).
- **App metadata + records** live in the shared SQLite DB, but every row is
  `owner_key`-scoped to exactly this session (so global queries stay safe).
- **File blobs** live only on disk under the session folder — the DB record stores the
  *relative path*, original filename, and content-type, never the bytes.
- Optional stretch for hard physical isolation: a per-session SQLite file
  `sessions/{sid}/app.db` mounted via `ATTACH`, so the session's app data isn't even in
  the shared DB file.

This is backward-compatible: the whiteboard loader already resolves `<dir>/{sid}.wb`; we
extend resolution to also check `sessions/{sid}/{sid}.wb`.

---

## 3. Storage schema — isolation-first

Two new tables only; existing whiteboard tables untouched.

```sql
CREATE TABLE IF NOT EXISTS wb_apps (
    board_id      TEXT PRIMARY KEY,      -- random secret, e.g. "b_9f3e…"
    owner_key     TEXT NOT NULL,         -- session id that created it
    title         TEXT NOT NULL,
    schema_json   TEXT,                  -- JSON Schema used to validate submissions
    public_reads  INTEGER NOT NULL DEFAULT 0,  -- 1 = anyone may list/get records+files
    created_at    TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wb_apps_owner ON wb_apps(owner_key);

CREATE TABLE IF NOT EXISTS wb_records (
    id          INTEGER PRIMARY KEY,
    board_id    TEXT NOT NULL,
    owner_key   TEXT NOT NULL,           -- owner-scoped admin/query
    seq         INTEGER NOT NULL,        -- per-board counter (sort key, newest first)
    payload     TEXT NOT NULL,           -- ANY JSON the board submitted
    created_at  TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_wb_records_board ON wb_records(board_id, seq);
CREATE INDEX IF NOT EXISTS idx_wb_records_owner ON wb_records(owner_key);
```

`board_id` is unguessable (128-bit random) — the capability token that makes cross-board
and cross-session access impossible even though the API is public.

---

## 4. Generic route family (one handler, many routes)

`POST /api/srv/{board_id}/submit` — **write** (public)
- body: arbitrary JSON (≤ 64 KB); validated against the board's stored `schema_json`
  (the `jsonschema` crate, incl. `format` assertion) when present; `201 { ok, seq }` |
  `400` invalid/schema-violation | `404` unknown.

`GET /api/srv/{board_id}/records?limit=&before=` — **list** (public only if `public_reads=1`, else owner/agent only)
- newest-first JSON array `[{ seq, payload, created_at }]`. Lets a board render its data.

`GET /api/srv/{board_id}/record/{seq}` — **single record** (same visibility as list)

`POST /api/srv/{board_id}/upload` — **file upload** (public)
- accepts **both** `multipart/form-data` (browser forms / `fetch` FormData — text fields plus
  one file part, merged into the record payload) and **raw-body** (binary + `X-Filename`,
  matching the existing `/api/upload`);
- blob written to `sessions/{sid}/apps/{board_id}/files/{file_id}.bin`;
  a record is inserted whose payload holds the form fields plus
  `{ file: "files/{file_id}.bin", name, type, size }`.
  `201 { ok, seq, file }`.

`GET /api/srv/{board_id}/file/{file_id}` — **serve blob** (same visibility as list)
- streams the stored file with its saved content-type.

Visibility rule: by default **reads are private** (only the owning session/agent, via the
MCP tool). A board that wants a public wall/gallery sets `public_reads=on` at creation;
the agent is the only party that can flip it (`srv apps update`).

---

## 5. Agent CLI — extend `manage_whiteboard` with the `srv` group

The MCP tool parses `wb --session <sid> <action> [args]`. Add a nested group so the
agent can create apps, **edit their schema**, and **query records/files** — all
owner-scoped (any command touching a board whose `owner_key != sid` is rejected).

### 5.1 Apps (registry)
```
wb --session <sid> srv apps create --title "<title>" [--json <schema>] [--public-reads on|off]
```
Creates a board, binds it to `sid`, returns the secret `board_id`.

```
wb --session <sid> srv apps list
```
Lists the session's boards: `board_id`, `title`, `record_count`, `public_reads`, `created_at`.

```
wb --session <sid> srv apps show <board_id>
```
Metadata + stored schema + public_reads for one board.

```
wb --session <sid> srv apps schema <board_id>
```
Prints only the stored JSON Schema (dedicated query — the missing tool).

```
wb --session <sid> srv apps update <board_id> [--title "<title>"] [--json <schema> | --clear-schema] [--public-reads on|off]
```
**Modify the schema** (or title / read visibility) of an existing board.

```
wb --session <sid> srv apps delete <board_id>
```
Deletes the board, its records, and its session-folder files.

### 5.2 Records
```
wb --session <sid> srv records submit <board_id> <json>
```
Insert one record (agent convenience mirroring the public endpoint).

```
wb --session <sid> srv records list <board_id> [--limit <n>] [--seq <start>]
```
Newest-first records, payload + seq + time.

```
wb --session <sid> srv records get <board_id> --seq <n>
```
One full record.

```
wb --session <sid> srv records query <board_id> '<$.path>' [--eq <value>]
```
**Query inside the JSON payload** via SQLite `json_extract`, e.g.
`query b_abc '$.email'` → every payload.email, or `--eq x@y.z` to filter. This is the
second missing tool: arbitrary queries against the serverless JSON.

```
wb --session <sid> srv records count <board_id>
wb --session <sid> srv records delete <board_id> [--seq <n> | --all]
```

### 5.3 Files
```
wb --session <sid> srv files list <board_id>
wb --session <sid> srv files get <board_id> <file_id> --out <path>
```
Owner-scoped access to the session-folder blobs (agent can fetch/verify uploads).

### 5.4 Ownership enforcement
Every `srv` command derives `owner_key` from `--session` and rejects any read/delete of
a board owned by another session. A session can never reach another session's data.

---

## 6. What the doc now covers that was missing

| Concern | Where |
|---|---|
| Modify/update the app JSON schema | §5.1 `srv apps update`, `--clear-schema` |
| Query the stored JSON schema | §5.1 `srv apps schema`, `srv apps show` |
| Query the records' JSON payload | §5.2 `srv records query` (`json_extract`) |
| Show DB content inside the whiteboard | §4 list/get routes + the read endpoints |
| File-upload support for forms | §4 `upload`/`file` routes, §2 folder layout |
| Per-session folder locality | §2 `<workspace>/sessions/{sid}/…` incl. app data + blobs |

---

## 7. Implementation steps

1. **Whiteboard file relocation** in `zw-whiteboard`: resolve `sessions/{sid}/{sid}.wb`
   (fall back to legacy `{sid}.wb` for existing sessions).
2. **Migration** in `zw-db` (`migrations.rs`): `wb_apps` + `wb_records` + indexes.
3. **Store** — new `serverless` module in `zw-db`: `srv_app_create/list/show/update/delete`,
   `srv_record_insert/list/get/query/count/delete`, each taking `owner_key` and enforcing
   the owner scope; blob path helpers resolving into the session folder.
4. **MCP CLI** — `zw-mcp/src/manage_whiteboard.rs`: add the `srv` group parser wired to
   the Store.
5. **HTTP handler** — `ch-web-ui`: one catch-all handler serving the §4 route family
   (submit/records/record/upload/file), reusing the Store.
6. **Doc** — this file.

Dependencies: `jsonschema` (validation; `default-features = false` to avoid its reqwest/rustls
defaults); multipart is hand-parsed; SQLite/libSQL already present. Keep modules small — one
concern per file, explicit imports, `cargo check` after edits.
