# Template Materials — Plan & Decisions

A **public, versioned, shared serverless-app store**. Any session can create a
template, embed it in a whiteboard/chat (shared live data), clone it into its own
space to edit, and commit improved versions back. Every reference is pinned to an
exact `{template_id}@{version}`.

MCP tool: `manage_materials`. CLI: `mat`.

---

## Status

- **Phase 1 — zw-db**: done. `templates` table in `SYSTEM_SCHEMA`, `templates.rs`
  (`TemplateRow`, `version_sid`/`parse_version_sid`, `tpl_create/get/list/set_version/remove`),
  `srv_app_create_with_id` added to `serverless.rs`.
- **Phase 2 — zw-materials crate**: done. `TemplateStore` over a second `SessionStore`
  rooted at `<whiteboard>/templates/`; version sid = `{tpl}@{ver}` = the board id inside
  each version DB. Modules: `layout.rs` (paths), `manifest.rs`, `snapshot.rs` (config +
  data + files copy), `diff.rs` (+`human_summary`), `store.rs` (head/version_pool/ensure/
  list/show/versions), `ops.rs` (publish/embed/clone/commit/update/remove),
  `query.rs` (info|schema|records|count|files|hooks|rate|ttl).
- **Phase 3 — manage_materials provider + `mat` CLI**: done. Mirrors
  `manage_whiteboard`; registered in `main.rs` (MCP-only + server mode).
  Full serverless coverage via `mat srv <group> <verb> <tpl>[@ver] [args]`:
  it reuses the existing `srv` engine (`execute_srv`) against the templates
  store with the version sid as both board id and owner key — so records
  CRUD + pagination (`list --limit/--offset`, `query --filter/--order/--limit/--offset`),
  full-text search (BM25), aggregate, hooks (register/list/remove/deliveries/replay),
  rate, ttl, links, audit, keys, outbound `call` (SSRF-guarded), and files
  all work pinned to the exact version. `apps create/delete` are locked (the
  version owns a single app); realtime WS for template boards serves through
  `/api/srv/{tpl}@{ver}/ws`.
- **Phase 4 — ch-web-ui / main**: done. `AppContext.materials`; `/materials/{material}`
  (JSON index) + `/materials/{material}/{file}` (read-only static serving);
  `/api/srv/{tpl}@{ver}/...` routes resolve to the version pool via `board_pool`.
- **Phase 5 — docs/help**: `mat about` + per-action `--help` shipped in
  `zw-mcp/src/materials_help.rs`; this doc tracks the plan.

Note: version boards use board id `{tpl}@{ver}` (not `tpl`), so the HTTP layer and the
version DB agree on identity. Clones are session boards `mat:{tpl}@{ver}` with meta at
`sessions/<sid>/materials/<tpl>@<ver>/mat.json`; clone files at `.../materials/<tpl>@<ver>/files`.

---

## 1. What it is

A template ("material") is a **full serverless app running at its own template id** —
same machinery as a session's serverless app (records, schema, hooks, rate, ttl,
keys, audit, files), but living in a **global public space** instead of inside one
session. Templates are **versioned** and **shared**: using one does not copy its
material, DB, or hook config into the session unless the session explicitly clones it.

Can hold almost any file type (cards, CSS themes, components, full apps, built
frontend `dist/`), as long as it works.

## 2. Why

- The whiteboard is session-private; its embedded HTML/CSS/JS is not reusable across
  sessions. Serverless apps are session-scoped too.
- There is no public place to publish a working app once and let every session use it
  without copying — with the ability to improve it and propagate versions safely.

## 3. Architecture

### 3.1 Storage — one global templates store

A second `SessionStore` rooted at `{workspace}/templates/`. Each version's sid is
`{template_id}@{version}`, giving version isolation in both DB and filesystem for free:

```
{workspace}/templates/
  sessions/{template_id}@{version}/
    {template_id}@{version}.db      # isolated DB: records, schema, hooks, rate, keys, audit
    apps/{board}/files/             # serverless files (same layout as session store)
  {template_id}/
    versions/{version}/
      manifest.json                 # name, kind, description, tags, capabilities, usage
      files/                        # material source (html/css/js/dist/...)
      changelog.md                  # why this version exists (see §6)
      diff.json                     # changes vs previous version (see §6)
  HEAD                              # {template_id} -> current version (also in system DB)
```

The system DB gains a `templates` index table:
`templates(template_id PRIMARY KEY, name, kind, description, current_version, created_at, updated_at)`.
All other serverless data stays in the per-version DBs.

### 3.2 Reuse of existing machinery

- `SessionStore` opens/caches per-version DBs exactly like per-session DBs.
- `srv_board_index` already maps board_id -> session; template boards register there.
- Serverless HTTP routes already work: `/api/srv/{template_id}@{version}/submit|records|ws|...`.
- Material files are served read-only at `/materials/{template_id}@{version}/{file}`.

## 4. Data semantics (decided)

1. **Embed (no clone)** = shared live data. Every session embedding `{tpl}@{v3}`
   reads/writes the same v3 DB. Nothing is copied anywhere.
2. **Clone** = copy material files + snapshot the DB into the session's own store
   (`{workspace}/sessions/{sid}/materials/{tpl}@{v}/...`), giving the session its own
   data and a working copy it can edit freely.
3. **Commit** = publish a new version (`vN+1`) from a tested clone: new immutable
   snapshot with the clone's files, config, and data. Must record *why* the clone was
   made and what changed vs the previous version (§6).
4. Write access to a published template's records is open to all authenticated
   sessions (no read-only default).
5. Future sessions embedding a version get that version's data.

## 5. Versioning

- Every external reference is pinned `{template_id}@{version}`.
- Versions are immutable snapshots; internal id is a monotonic per-template counter.
- Publishing a new version never breaks existing embeds/clones — they stay on their
  pinned version until an explicit `mat update --to @{ver} --into <sid>`.
- `HEAD` points at the current version; `templates.current_version` mirrors it.

## 6. Commit change log (decided)

`mat commit` records:

- **`changelog.md`** — free-text: why the clone was made and committed.
- **`diff.json`** — machine-readable diff vs the previous version:
  - `files`: added / modified / removed (list of paths + per-file status)
  - `config`: schema, hooks, rate, ttl, public_reads, unique changes
  - `data`: summary of record changes (added N, removed M, updated K) — structural
    diff of data, not raw blobs; exact payload diff available on request via `query`
- Commit publishes the new snapshot from the clone's state (files + config + data).

## 7. MCP tool: `manage_materials`

Mirrors `manage_whiteboard`: string-command CLI, `--help` + per-action help,
JSON status/error envelopes, `command` input schema, never the shell tool.

CLI verbs:

```
about
list                                   list templates + current versions
show {tpl}@{ver}                       manifest, capabilities, changelog
create [--name N] [--kind K]           create template space (v1)
publish <sid> <app>                    promote a session's working app to a template (v1)
embed {tpl}@{ver} --into <sid>         reference only; shared live data; no copy
clone {tpl}@{ver} --into <sid>         copy files + snapshot DB into the session
commit <sid> {tpl} --why "<note>"      publish vN+1 from a tested clone (files+config+data+changelog)
versions {tpl}                         list versions
update --to @{ver} --into <sid>        move an embed/clone to a new version
query {tpl}@{ver} <records|files|schema|hooks|rate|ttl|keys|audit ...>
                                        operate on the exact version only
remove {tpl}                           remove template + all versions
```

All tools must target the exact `{template_id}@{version}` and touch only that
version's DB and files.

## 8. Implementation phases

1. **zw-db**: `templates` table in SYSTEM_SCHEMA + migration; version sid helpers.
2. **zw-materials** crate: template store (files+manifest layout, HEAD, changelog,
   diff builder), create/publish/embed/clone/commit/update/query operations.
3. **manage_materials** MCP provider + `mat` CLI in the crate (mirrors
   `manage_whiteboard` / `srv` pattern, per-action help).
4. **ch-web-ui / main**: register provider; `/materials/{tpl}@{ver}/{file}` static
   serving; clone material serving under `sessions/{sid}/materials/`.
5. **Docs/help**: `mat about` capability text; per-action `--help`.

Each phase verifies with `cargo check` only (never `cargo build`/`cargo clean`).

## 9. Open items

- Whether `publish` should auto-version existing templates or always require
  clone -> commit for changes (lean: publish = new template only; changes via commit).
- Data diff exactness on large record sets (summary vs full payload).
