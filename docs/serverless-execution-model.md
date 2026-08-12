# Serverless Execution Model — From "Code" to Config + Rust Primitives

> This doc answers one question: **how do we cover everything general-purpose code
> execution (Lambda / Cloudflare-Workers style) offers, without shipping a user-code
> sandbox?**
>
> The answer: keep the model "**configuration + safe Rust primitives**" and close the
> remaining gaps with two new Rust crates:
>
> 1. **`zw-expr`** — a safe, side-effect-free expression / computed-field engine.
> 2. **`zw-recipe`** — a declarative rule/recipe engine (event → condition → actions)
>    with a secrets layer.
>
> Arbitrary user code stays out of scope (see §7). Everything below is expressed with
> existing primitives — filters, patch ops, joins, cron, outbound calls, webhooks, keys,
> rate limits, audit — orchestrated by new, small, well-tested Rust modules.

---

## 1. Guiding model

User code on a platform like AWS Lambda lets you do *anything*, but also makes you own
*everything* (sandbox, resource limits, network policy, secrets, security).

Our model gives up the "anything" and keeps the 90% that apps actually need, expressed
as **declarative config** interpreted by safe Rust:

```
┌─ App config (wb_apps + new recipe/expr tables) ──────────────┐
│  schema, keys, rate, ttl, audit, link, hooks, jobs          │
│  recipes: [ {when, match, actions} ]                        │
│  computed: { total: "$.price * $.qty" }                     │
│  secrets: { STRIPE: "…" } (masked, ${VAR} substitution)     │
└───────────────┬─────────────────────────────────────────────┘
                ▼
      Rust engine (reads config, applies primitives)
        • filters  • patch ops  • joins  • cron
        • outbound /call  • webhooks  • upsert  • audit
                ▼
      wb_records + effects + audit + realtime events
```

Nothing user-authored is executed; it is *data* that the engine interprets.

---

## 2. Coverage catalogue — every "code execution" capability mapped

Legend for the **How** column:
- **✅** already implemented
- **🔵** planned via `zw-expr` / `zw-recipe` (detailed in §4–§5)
- **⏭** intentionally out of scope (§7)

### 2.1 Triggers — what *fires* logic

| Capability code-execution gives | How we cover it in Rust/config | Status |
|---|---|---|
| On record create / update / delete | **Recipe engine**: `recipes:[{when:"record.created", match, actions}]`; the same events already drive WS/SSE/webhooks | 🔵 |
| On a schedule | Cron jobs (`app_jobs` + 5-field/`@every` solver) | ✅ |
| On inbound HTTP / webhook POST | New `POST /api/srv/{board}/events` receiver that feeds recipes | 🔵 |
| On outbound call completion | A job/action may chain a follow-up recipe (simple sequencing) | 🔵 |
| Retry with backoff | Webhook backoff exists; extend to recipe actions | ✅ / 🔵 |

### 2.2 Transformations & computed data (the biggest reason people write code)

| Capability | How we cover it | Status |
|---|---|---|
| Field math / string / date logic | **Safe expression grammar** in `zw-expr` (`$.price * $.qty`, `lower($.name)`, `daysAgo($.at)`, `if(a,b,c)`) | 🔵 |
| Computed / derived fields | Config `computed: { total: "<expr>" }`, evaluated on read or materialized on write; usable in filter/sort/search | 🔵 |
| Cross-field validation beyond JSON-schema | `validate: [{ when: "<expr>", error: "…" }]` | 🔵 |
| Redact / sanitize sensitive fields | `redact: ["$.ssn"]` — masked on read for non-owner keys | 🔵 |
| Copy / move / reshape fields | Recipe ops `$copy`, `$move`, `$format`, `$merge` | 🔵 |
| Array manipulation | Recipe / patch ops `$push`, `$pull`, `$sort`, `$slice` (patch already has `$set/$inc/$dec/$mul/$unset`) | 🔵 |

### 2.3 Routing, branching, workflows

| Capability | How we cover it | Status |
|---|---|---|
| If / else business rules | Recipe `match:` guard clauses + per-step conditions (`when`) | 🔵 |
| Multi-step workflows | Recipes are ordered steps; persistent per-record `state` field | 🔵 |
| State machine | `states` + allowed transitions + on-transition actions (config) | 🔵 |
| Fan-out / parallel | Sequential recipe steps; parallelism via separate rules — rarely needed | 🔵 |
| Dedup / idempotency | `dedup_on: "<expr>"` per action; upsert already dedups on `unique_key` | 🔵 / ✅ |
| Cross-board read & write | Extend joins (read-only today) to **copy/move/sum into another board** recipe actions | 🔵 |

### 2.4 I/O & integrations

| Capability | How we cover it | Status |
|---|---|---|
| Call external APIs | `/api/srv/{board}/call` + `jobs{type:"http"}` (SSRF-guarded) | ✅ |
| Send webhooks | Webhook registry + deliveries (retry/dead-letter/replay) | ✅ |
| Send email / SMS / chat | Outbound call to a provider; **secret substitution** holds creds server-side | ✅ / 🔵 |
| Enqueue deferred work | Recipe action `schedule(name, cron/nD, action)` reuses the job engine | 🔵 |
| Files / objects | File list exists; per-record attachment refs + access URLs | 🔵 |

### 2.5 Ops & observability (code platforms give these "for free")

| Capability | How we cover it | Status |
|---|---|---|
| Secrets management | `secrets` map on app config; masked on read; `${VAR}` substitution in hooks/calls/jobs/actions | 🔵 |
| Dry-run | `?dry_run=true` on recipes/bulk: report changes without applying | 🔵 |
| Replay | Webhook replay exists; extend to re-running a past event through rules | ✅ / 🔵 |
| Logs / traces | Audit trail logs record + key events; add `recipe_run` events | 🔵 |
| Timeouts / budgets | Per-action timeouts + per-board execution budget (rate-weighted) | 🔵 |

### 2.6 Stability / safety guarantees (why no sandbox is needed)

| Guarantee | Mechanism |
|---|---|
| No user code can memory-safety / panic the process | Expressions are **data**, not code; evaluation is pure Rust with hard caps |
| Bounded CPU / time | `zw-expr` is polynomial-time, iteration-capped, side-effect free |
| Bounded network | reuse existing SSRF guard for every outbound primitive |
| Access control | keys/roles + ownership checks apply to every new surface |
| Auditable | every recipe run writes an audit row |

---

## 3. Crate layout (both are NEW crates in the workspace)

```
zerowrapper/
├── zw-expr/            # NEW — pure expression / computed-field engine (no DB)
│   └── src/
│       ├── lib.rs      # parse + eval entry points
│       ├── ast.rs      # typed AST (literals, refs, unary/binary, calls, ternary)
│       ├── parser.rs   # Pratt/top-down parser → AST  (errors, no panics)
│       ├── eval.rs     # interpreter over a JSON-path resolver + data context
│       └── funcs.rs    # built-in function table (string/num/date/aggregate)
│
└── zw-recipe/          # NEW — rule engine + secrets (depends on zw-db primitives)
    └── src/
        ├── lib.rs      # orchestration entry (run_rule, apply_actions)
        ├── rule.rs     # recipe config types + validation
        ├── actions.rs  # $compute/$copy/$upsert_other/$notify/$call/$schedule/…
        ├── context.rs  # event + record context, per-run budget/timeout
        ├── secrets.rs  # secrets store, ${VAR} substitution, redaction
        └── events.rs   # inbound event receiver ingestion
```

Rules:
- **`zw-expr`** is a leaf crate: no `zw-db`, no I/O — trivially testable, cannot panic on
  hostile input.
- **`zw-recipe`** depends on `zw-db` (filters, patch ops, joins, cron, `/call`, hooks) and
  `zw-expr`. It holds no global state; every run is driven by a passed context.
- Both are added to the workspace `[members]` and linked from `ch-web-ui` (scheduler
  worker + event receiver) and `zw-mcp` (CLI subcommands).

---

## 4. Feature 1 — Safe expression & computed-field engine (`zw-expr`)

### 4.1 Purpose
Give config the power of "computed fields / small logic" that users normally write as code,
**without** a general interpreter.

### 4.2 Grammar (design sketch)
- **Literals**: numbers, strings (`"…"`, `'…'`), booleans, `null`.
- **JSON-path refs**: `$.price`, `$.items[1].qty`, `$` (whole payload), `$.meta.*`.
- **Arithmetic**: `+ - * / %` and integer/float aware.
- **Comparison / logic**: `== != < <= > >= && || !`.
- **Ternary**: `cond ? a : b`.
- **Calls**: `lower(s) upper(s) trim(s) len(x) num(x) str(x) join(arr,sep) round(x,n) floor(x)
  ceil(x) abs(x) min(a,b) max(a,b) daysAgo(ts) nowUnix() concat(a,b) contains(x,sub) startsWith
  endsWith if(c,a,b) coalesce(a,b)`.
- **Grouping**: parentheses.

### 4.3 Guarantees (the whole point)
- **Pure**: eval never mutates inputs, never performs I/O, never sleeps.
- **No panics**: env can't affect parser/eval; errors return `ExprError`, iteration is
  depth-capped; input size-capped.
- **Bounded**: recursion depth and container node counts are capped; `*` expansion capped.

### 4.4 Where it plugs in
- **Computed fields**: `computed: { total: "<expr>", slug: "lower(trim($.name))" }` — evaluated
  on *read* (and optionally materialized on write). Computed fields are exposable in
  filter/sort/search/aggregate.
- **Validation**: `validate: [{ when: "<expr>", error: "msg" }]` layered over JSON-schema.
- **Recipe conditions**: `match:` and per-step `when:` accept expressions.

### 4.5 Example
```
computed:
  total:      "$.price * $.qty"
  in_stock:   "if($.stock > 0, true, false)"
  label:      "concat(upper($.brand), ' | ', $.name)"
validate:
  - when: "$.qty >= 0"
    error: "qty must not be negative"
```

---

## 5. Feature 2 — Recipe / rule engine + secrets (`zw-recipe`)

### 5.1 Recipe config shape
```json
{
  "when": "record.created",              // or "record.updated" | "record.deleted" | "inbound" | "cron:<name>"
  "match": { "status": { "eq": "paid" } },  // existing filter (optional)
  "name": "send-receipt",                // human label
  "enabled": true,
  "dedup_on": "$.order.id",              // optional idempotency
  "actions": [
    { "$compute": { "total": "$.price * $.qty" } },
    { "$notify": { "url": "https://hook.example/rc", "secret": "STRIPE", "payload": "{{$}}" } },
    { "$upsert_other": { "board": "./invoices", "via": "$.sku", "payload": {"sku": "$.sku", "total": "$.total"} } },
    { "$call": { "url": "https://api.example/…", "method": "POST", "headers": [], "body": "{{$}}", "secret": "API_KEY", "when": "$.amount > 100" } },
    { "$schedule": { "name": "recheck", "at": "@every 1h", "action": {"type":"poll","url":"…","path":"$.v"} } }
  ]
}
```

### 5.2 Action catalogue
| Action | Effect |
|---|---|
| `$compute` | evaluate `zw-expr` fields into the record (in place) |
| `$copy` / `$move` / `$format` / `$merge` | reshape the payload |
| `$push` / `$pull` / `$sort` / `$slice` | array transforms |
| `$upsert_other` / `$move_to` | write into *another* board (joins + write-through) |
| `$notify` | webhook / SSE / call a URL with a payload |
| `$call` | SSRF-guarded outbound request (headers may `secret`-substitute) |
| `$schedule` | enqueue a deferred job (reuses `app_jobs`) |
| `$set_state` | advance a persisted state machine |
| `$log` / `$audit` | write a recipe-run audit row |

### 5.3 Execution context & safety
- Each `when` event builds an **immutable context** (event type, board, record seq/payload,
  actor). No cross-request shared mutable state.
- Per-run **budget**: max actions, max per-action timeout (reuse `/call` timeout clamp),
  cumulative board quota honored via existing rate limiter.
- Every outbound call goes through the **SSRF guard**.
- A rule that throws stops the run, records the failure, and (optionally) re-enters for retry.

### 5.4 Secrets
- `secrets: { NAME: "<value>" }` in app config.
- Never returned in read responses — masked as `{"set": true}`.
- Substitution syntax `${NAME}` (or `{{NAME}}`) inside hook/call/job/action `headers`, `body`,
  `url`, `secret`.
- Owners set/rotate/remove secrets via CLI (`srv apps secrets set/get/remove <board>` and HTTP).
- Secrets are hashed at rest in the system DB (never plaintext), decrypted only in the engine.

### 5.5 Dry-run, replay, idempotency
- `?dry_run=true`: evaluate `match` + actions and report would-be changes (plus `dedup_on`
  key conflicts) without applying.
- Replay: re-submit a historical `record.*` event id through rules (reuses audit/event log).
- Idempotency: `dedup_on` prevents duplicate effects for the same logical key.

---

## 6. Final coverage matrix (target state)

| Scenario bucket | Covered by |
|---|---|
| Persist / validate / query / sort / search / aggregate | Base + filters + FTS + `zw-expr` computed fields |
| Auth, keys, roles, rate limits, TTL, audit, joins, export/import | Existing |
| Realtime + webhooks + cron + outbound + inbound events | Existing + `zw-recipe` |
| On-event automation, transforms, cross-board writes, secrets, dry-run, replay | `zw-recipe` + `zw-expr` |

## 7. Deliberately out of scope
- Arbitrary user-defined functions / loops / recursion escape hatches.
- Interactive shells, dynamic `eval`, plugin loading from app input.
- Unbounded memory/CPU, sub-second latency guarantees.
- A general-purpose third-party package manager.

These would reintroduce the sandbox problem we deliberately avoid.

---

## 8. Implementation steps

1. **`zw-expr` crate** — AST → parser → eval (pure, no deps on `zw-db`); unit tests for
   grammar, precedence, functions, and hostile-input non-panics. `cargo check` only.
2. **Computed + validate hooks** in `zw-db` serverless read/write path (optional fields on
   `wb_apps`, delegated to `zw-expr`).
3. **`zw-recipe` crate** — rule/action engine + secrets store; unit + integration tests
   (via an in-memory/DB harness similar to `zw-db/tests/*`).
4. **Wiring** — scheduler worker + `POST /events` receiver in `ch-web-ui`; CLI subcommands
   in `zw-mcp` (`srv apps recipes …`, `srv apps secrets …`, `srv apps compute …`).
5. **Docs + help updates** — this doc, `serverless-whiteboard-apps.md` §9, and MCP help.
6. **Security pass** — review secrets-at-rest, SSRF reuse, budgets, audit, ownership on
   every new endpoint.

Dependencies: `zw-expr` is standalone; `zw-recipe` adds `zw-db` + `zw-expr` to the workspace.
Keep every module < ~200 lines, explicit imports, run `cargo check` after each edit.
