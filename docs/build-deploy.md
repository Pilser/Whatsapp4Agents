# Build & Deploy Guide (waos / zerowrapper)

How to build the frontend + Rust service and deploy it to the Docker-backed sites
(`vox` on :9090, `pilserlabs` on :9091).

## Prerequisites

- `bun` — bundles the frontend JS.
- `cargo` (Rust toolchain) — builds the `zw` release binary.
- `topcoat` (`~/.cargo/bin/topcoat`) — extracts hashed assets from the binary into a
  directory the containers serve.
- `docker` + `docker compose` — run the containers.

Key paths:

- Repo: `/home/vox/.AAAPROJECTS/Whatsapp4Agents`
- Crate: `zerowrapper/crates/ch-web-ui`
- JS entry: `zerowrapper/crates/ch-web-ui/assets/js/chat/app-entry.js`
- Deploy roots: `/home/vox/pilserlabs/vox` and `/home/vox/pilserlabs/pilserlabs`
- Compose + env (tracked in this repo): `zerowrapper/deploy/{compose.yml,vox.env,pilserlabs.env}`

## Two-part build

The project is **Rust (topcoat) + JS (bundled by bun)**. The Rust binary registers the JS
asset at compile time; the JS must be bundled by bun *before* `cargo build --release`.

### 1. Bundle frontend JS (bun) — only if you changed JS/CSS

```bash
cd zerowrapper/crates/ch-web-ui/assets/js/chat
bun build app-entry.js \
  --format esm \
  --external marked \
  --external highlight.js \
  --external katex \
  --external mermaid \
  --outfile app.js
```

CDN libraries (marked, katex, mermaid, highlight.js) stay external via the importmap; the
output `app.js` is what the server serves.

Optional: after editing any Rust file, sanity-check with `cargo check` (fast).

### 2. Build the release binary (cargo)

```bash
cd zerowrapper
cargo build --release
```

This embeds the current `app.js`/`styles.css` into the `zw` binary's asset registry.

## Deploy

### 3. Bundle assets to the deploy dirs (topcoat)

`topcoat` reads the release binary's asset registry and writes hashed files (e.g.
`app-<hash>.js`) + `manifest.toml` into each site's assets dir, which the container mounts
read-only as `/app/config`.

```bash
cd zerowrapper
topcoat asset bundle --bin zw --release --out /home/vox/pilserlabs/vox/assets
topcoat asset bundle --bin zw --release --out /home/vox/pilserlabs/pilserlabs/assets
```

### 4. Copy the binary (atomic, safe while containers run)

Copy to a `.new` file then `mv` so the running process keeps its old inode safely:

```bash
cp target/release/zw /home/vox/pilserlabs/vox/zw.new
cp target/release/zw /home/vox/pilserlabs/pilserlabs/zw.new
mv -f /home/vox/pilserlabs/vox/zw.new        /home/vox/pilserlabs/vox/zw
mv -f /home/vox/pilserlabs/pilserlabs/zw.new /home/vox/pilserlabs/pilserlabs/zw
```

### 5. Recreate the containers (docker compose)

`--force-recreate` is required because the bind mounts changed without a compose config
change:

```bash
cd zerowrapper/deploy
docker compose -f compose.yml -p vox \
  --env-file vox.env up -d --force-recreate

docker compose -f compose.yml -p pilserlabs \
  --env-file pilserlabs.env up -d --force-recreate
```

### 6. Verify

```bash
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:9090/   # vox: 200
curl -s -o /dev/null -w "%{http_code}\n" http://localhost:9091/   # pilserlabs: 200
curl -s http://localhost:9090/ | grep -o 'app-[a-f0-9]*\.js' | head -1   # current asset hash
```

Assets are served under `/_topcoat/assets/` (not the site root). To confirm a change is
live, download that asset and grep for the new JS:

```bash
curl -s http://localhost:9090/_topcoat/assets/<app-hash>.js | grep '<your-symbol>'
```

## Troubleshooting

- **Frontend change shows old behavior after deploy**: hard-refresh the browser (assets are
  hash-versioned, so a refresh usually suffices). Confirm the served asset via step 6.
- **Scripts/HTML not executing in chat**: script execution (`runScripts=true`) is currently
  enabled for the **whiteboard body only**, not chat messages. Enable it in `messages.js`
  and `owner-chat.js` if agent widgets need to run in chat.
- **`docker compose up -d` reports "Running" without recreating**: add `--force-recreate`.