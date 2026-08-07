# Serverless Infra & Account-Layer Roadmap

These are the features that **cannot live inside the per-board data store alone**.
They need an identity/account layer, host middleware, durable background jobs, or
cross-process coordination. The data store (turso, per-owner DB files) from
`zerowrapper/zw-db` stays as-is; each item below defines the *externalfeature*,
its API surface, and the pieces that have to be added around it.

Ordering hint: build **F0 (keys/auth)** first — everything else depends on
verifying *who* is writing, so we can scope limits, allowlist webhooks, and
identify subscribers.

---

## 0. API keys & per-writer accounts (foundation)

**Scenario gap:** today *everyone* who knows a `board_id` can write `/submit`.
Apps that are more than a shared guestbook need: per-user writes, per-user
limits, revocable access, and server-trustworthy identity for the later
features.

**Target API (host level, auth middleware shared by all routes):**
- `POST /api/srv/<board>/keys` (admin key) -> issues an app key with a role
  (reader | writer | admin) and an optional fixed `writer` id.
- Requests then send `Authorization: Bearer <app_key>`; the `board_id` is
  derived from the key, not trusted from the URL. `X-` optional `X-Writer`
  header is *replaced* by the key's own writer identity (no client forgeries).
- Admin rotation/revoke: `CLI: srv apps keys list | issue | revoke`.

**Store/model (still in turso DB):**
- New tables (added by a migration, no new infra): `wb_keys(bucket, board_id,
  key_hash, role, writer, created_at, revoked_at)`. Store only a salted hash
  of the secret; the secret is shown once at issue.
- Migration from the old model: the legacy `board_id` acts as an unscoped
  `admin` key, so existing boards keep working.

**Acceptance:** issuing a writer key, writing with it tags records with its
`writer` (cannot be spoofed via `X-Writer`); revoking a key immediately rejects
it; `GET /records` with a reader key works, without it returns 403.

---

## 1. Rate limiting & abuse protection

**Scenario:** public/private boards: from qr polls, forms, up-vote widgets,
uploads. Bot floods should back off, not fill the DB.

**API:**
- Per-app, per-key and per-IP sliding-window bucket limits, configurable from
  the CLI: `srv apps rate <board> '{"submit":60,"upload":10,"search":120,"per_day":1000}'`
- Violations return `429`; response includes `Retry-After` and a
  `X-RateLimit-Remaining` per window.
- Global (per-IP) defaults apply even to boards that never configured limits,
  so defaults are sane without config.

**Pieces:**
- A middleware that runs BEFORE each `/api/srv/*` handler. Buckets live in
  memory at boot (fast) and are backed by the owner DB for consensus per site.
- Identity is `key_hash` when Authorization is present, else source IP.

**Acceptance:** spamming submit 61 times inside one minute → the 61st gets 429
with `Retry-After`; a revoked but rate-limited key still can't write after
revoke.

---

## 2. Realtime (pub/sub) for boards

**Scenario:** guestbooks, dashboards, chat-lite, multiplayer counters. Pages
should react to new data without polling.

**What:**
- `GET /api/srv/<board>/ws` (WebSocket upgrade, or SSE fallback
  `?events=1`). Optional `filter=<json>` + `after=<seq>` for catchup.
- Server pushes JSON events on every insert/update/delete, one event =
  `{"type":"record.created","seq":5,"record":{...}}` (+ `record.updated`,
  `record.deleted`).
- Private boards require a writer/reader key; public boards are open.

**Pieces:**
- The host process already owns the single DB connection; hook the
  write-path (insert/set/patch/delete) to also publish to an in-process
  broadcast channel, then flush to the WebSocket fan-out. Catch-up by
  `seq > last` on (dis)connect.
- Subscriptions are ephemeral (no durable queue) v1; a durable ring buffer
  (last 1000 events) covers the reconnect case.

**Acceptance:** two browser tabs; one submits, the other updates within ~1s
with no polling; a tab that was offline receives missed events on reconnect.

---

## 3. Webhooks (server-side actions)

**Scenario:** an app needs a side-effect when data changes — notify an owner's
remote service, clean a cache, update an external catalog, trigger an agent
later.

**What:**
- `POST /api/srv/<board>/hooks` with a URL (+ optional HMAC secret) is
  registered/removed per app; webhooks fire on `record.created`, `.updated`,
  `.deleted`.
- Outbound call: `POST <url>` JSON body
  `{"board","event","seq","record","hooks_secret"}` signed via
  `X-WAOS-Signature` (HMAC-SHA256 of body with the hook secret) so the target
  can verify origin.
- Delivery guarantees: immediate attempt → retry with backoff (0s, 1s, 5s,
  30s, 2m, up to 5 times), then dead-letter (store the failed deliveries for
  replay). `srv apps hooks list | replay <delivery_id> | remove <url>`.

**Pieces:**
- A durable delivery table (same owner DB) as the queue, a background worker
  that retries with `Retry-After`, and outbound HTTP from the host process.
- Signing/verify helper shared with docs.

**Acceptance:** registering a hook receives a call for each insert, updated
correctly; a dead URL gets a dead-lettre and `replay` re-delivers it after the
service is back.

---

## 4. TTL / record expiry

**Scenario:** offers, config, queue items, one-time-consume tokens — data that
should disappear on its own.

**What:**
- App-level: `srv apps ttl <board> <seconds>` → a sweeper deletes records
  older than N seconds (checked on boot and periodically; also filtered
  lazily on reads so a stalled sweeper never leaks expired rows).
- Field-level: `--ttl-until '<$.expires_at>'` reads that JSON timestamp.
  Missing/absent field = never expires.

**Pieces:**
- A sweeper: `DELETE ... WHERE board_id=? AND created_at < now-ttl` (or
  `json_extract(payload, expires) < now`), periodic background task; an
  index on the expiry expression keeps it cheap.
- Flag TTL changes on `apps update` so boards can be tuned live.

**Acceptance:** a record inserted with a 5s TTL disappears from reads and
store after ~6s; field-based TTL honors `expires_at`; changing TTL applies to
new inserts only.

---

## 5. Cross-board joins

**Scenario:** two apps splitting concerns — `products` app, `orders` app
referencing a product by id. Reads should return the parent object resolved
server-side (when it exists).

**What:**
- `apps link <childBoard> --on '<$.sku>' --to <parentBoard> --via '<$.sku>'`
  registers a join. Server records the join cfg (in the owner DB).
- New `GET /api/srv/<child>/join` returns each child record with
  `joined.<fromKey>` = the matched parent record payload. Reads require
  public-reads on EITHER both boards (public path) or a key with both.
- Empty joins are `joined: null` (optional `--strict` drops them from the
  result).

**Pieces:**
- The existing store query function + a small in-memory registry of joins
  built from the wb_apps/`wb_links` table at boot.
- A note on N+1: resolve joins with one `IN (...)` query per child page, not
  a join per row, for page ≤ 500.

**Acceptance:** `orders` records come back with the `product` object embedded
from the `products` app by matching `sku`; filtering still works on child
fields; a dangling sku yields an empty join.

---

## 6. Audit log & ownership evidence (optional but cheap)

**Scenario:** for real rolled-out "shopping" or "survey" apps, admins want a
copy of create/update/delete events + who did it.

**What:**
- Per board an append-only `wb_audit(board, seq, event, key_actor, writer,
  payload_hash, ts)` written in the same tx as the mutation when the app has
  `audit: on`.
- `srv records audit <board> [--since ISO] [--limit`]; reader=admin/owner only;
  exposed as `GET /.../audit` behind a key.

**Acceptance:** every mutation is traceable to key+writer+time; audit cannot
be written directly via submit, only via the audit stream.

---

## Suggested delivery sequence
1. **Keys/auth** (unlocks everything else).
2. **Rate limiting** (public boards stop being abusable).
3. **Realtime** (highest user-visible value for whiteboard embedded apps).
4. **Webhooks**, then **TTL**, then **joins**, then **audit**.

Each is independent of the others and can be developed/tested/deployed as a
feature branch with the same test/deploy loop used for the store features.