# Build & Deploy Guide (waos / zerowrapper)

How to build the Rust service and deploy it to the Docker-backed sites
(`vox` on :9090, `pilserlabs` on :9091).

## Prerequisites

- `cargo` (Rust toolchain) — builds the `zw` release binary.
- `topcoat` (`~/.cargo/bin/topcoat`) — extracts hashed assets from the binary into a
  directory the containers serve.
- `docker` + `docker compose` — run the containers.
- `bun` — only needed when changing the frontend JS/CSS (see the two-part build below).

Key paths:

- Repo: `/home/vox/.AAAPROJECTS/Whatsapp4Agents`
- Crate: `zerowrapper/crates/ch-web-ui`
- JS entry: `zerowrapper/crates/ch-web-ui/assets/js/chat/app-entry.js`
- Deploy dir (shared binary + assets, gitignored): `zerowrapper/deploy/{zw,assets/}`
- Per-domain config (tracked): `zerowrapper/deploy/config/{vox,pilserlabs}/`
- Compose + env (tracked in this repo): `zerowrapper/deploy/{compose.yml,vox.env,pilserlabs.env}`
- Runtime data (not in repo): `/home/vox/pilserlabs/{vox,pilserlabs}/data`

## Layout

The two containers share one binary (`zw`) and one asset bundle (`assets/` — the hashed
`.js`/`.css` + `manifest.toml`, identical for both sites). Each site only mounts its own
per-domain files (`favicon.svg`, `robots.txt`, `landing.yaml`, `sitesettings.yaml`) as
`/app/config`:

| env var                  | vox                       | pilserlabs                   | mounted as        |
|--------------------------|---------------------------|------------------------------|-------------------|
| `ZW_BIN`                 | `deploy/zw`               | `deploy/zw`                  | `/usr/local/bin/zw` (ro) |
| `BUNDLE_DIR`             | `deploy/assets`           | `deploy/assets`              | `/app/bundle` (ro) |
| `CONFIG_DIR`             | `deploy/config/vox`       | `deploy/config/pilserlabs`   | `/app/config` (ro) |
| `TOPCOAT_ASSETS_DIR`     | `/app/bundle`             | `/app/bundle`                | —                  |
| `ZEROWRAPPER_CONFIG_DIR` | `/app/config`             | `/app/config`                | —                  |
| `ZEROWRAPPER_LANDING_YAML`| `/app/config/landing.yaml`| `/app/config/landing.yaml`  | —                  |
| `ZEROWRAPPER_SETTINGS_YAML`| `/app/config/sitesettings.yaml`| `/app/config/sitesettings.yaml`| —          |

The compose project name comes from `INSTANCE_NAME` in each env file (top-level
`name: ${INSTANCE_NAME}`), so the true project names are `vox` and `pilserlabs` — matching
the env files. No `-p` flag is needed.

`DIST_DIR` / `ZEROWRAPPER_CH_WEB__DIST_PATH` (the old `web/dist` static frontend) have been
removed — the topcoat bundle under `TOPCOAT_ASSETS_DIR` is the only frontend served.

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

### 3. Bundle assets to the shared deploy dir (topcoat)

`topcoat` reads the release binary's asset registry and writes hashed files (e.g.
`app-<hash>.js`) + `manifest.toml` into the shared bundle dir, which both containers mount
read-only as `/app/bundle`:

```bash
cd zerowrapper
topcoat asset bundle --bin zw --release --out /home/vox/.AAAPROJECTS/Whatsapp4Agents/zerowrapper/deploy/assets
```

The per-domain files (`favicon.svg`, `robots.txt`, `landing.yaml`, `sitesettings.yaml`)
live under `zerowrapper/deploy/config/{vox,pilserlabs}/` and are edited in place (tracked
in git); they are NOT part of the bundle.

### 4. Copy the binary (atomic, safe while containers run)

Copy to a `.new` file then `mv` so the running process keeps its old inode safely:

```bash
cd zerowrapper
cp target/release/zw deploy/zw.new
mv -f deploy/zw.new deploy/zw
```

Both containers mount this same file, so one copy updates both.

### 5. Recreate the containers (docker compose)

`--force-recreate` is required because the bind mounts changed without a compose config
change. The project name is taken from `INSTANCE_NAME` in the env file:

```bash
cd zerowrapper/deploy
docker compose -f compose.yml --env-file vox.env up -d --force-recreate
docker compose -f compose.yml --env-file pilserlabs.env up -d --force-recreate
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