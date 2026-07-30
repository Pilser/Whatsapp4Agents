# WAOS Project

This is a Rust-based project in `zerowrapper/`. There is a reference frontend at `zerowrapper/web/` (Astro + JS) which is being replaced by a new Topcoat-based crate at `zerowrapper/crates/ch-web-ui/`. Reference implementations exist in `whatsapp-rust/` and `zerocrow/` folders.

## RULES

### Build commands
- **Only `cargo check`** for verifying compilation during development
- **Only `cargo build --release`** when a release binary is needed
- **Never** run `cargo clean` or delete the `target/` directory
- Run `cargo check` after editing any Rust file
- For the existing `web/` frontend, use `bun run build`

### Workflow
- When you detect you are given a summary from a recent session, do NOT rush to implement in code. First verify with the user and get a go-ahead.
- Use sub-agents for implementing complex multi-file changes.
- Read the plan at `WAOS_WEB_REPLACEMENT_PLAN.md` before working on `ch-web-ui`.

### Code style
- One concern per file, max ~200 lines per file
- Explicit imports only (no `use super::*`)
- No comments unless the code is genuinely unclear
- Follow the existing module structure in the crate
