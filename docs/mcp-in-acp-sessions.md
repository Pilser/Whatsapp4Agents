# MCP Tools in Daemon/ACP Sessions

## Problem

MCP tools work in interactive CLI/TUI sessions (`zeroclaw agent --agent craw`) but **not in daemon sessions** originating from ACP-over-stdio or ACP-over-WebSocket.

> **Note for zerowrapper users**: The RPC socket path (Unix socket JSON-RPC) already initializes MCP because it defaults to `ChatMode::Chat`. If MCP was not working from zerowrapper, the fix was simply **restarting the daemon** after ensuring `[mcp] enabled = true` in config — no code changes needed for the RPC path.

The root cause is in **two separate code paths** for session creation:

### 1. RPC Socket Path (zerowrapper, waos, TUI/zerocode)

zerowrapper connects via Unix socket + JSON-RPC (`session/new`). The handler in `zeroclaw/crates/zeroclaw-runtime/src/rpc/dispatch.rs:812`:

```rust
let initialize_mcp = session_should_initialize_mcp(&chat_mode);
```

Where `session_should_initialize_mcp` returns:
- **`true`** for `ChatMode::Chat` (default when client omits `chat_mode`)
- **`false`** for `ChatMode::Acp`

The RPC path defaults to `Chat` when no `chat_mode` is sent → **MCP IS initialized** here.

### 2. ACP Path (stdin/stdout ACP, ACP-over-WebSocket)

The ACP server in `zeroclaw/crates/zeroclaw-channels/src/orchestrator/acp_server.rs` calls `Agent::from_config_with_session_cwd_and_mcp_backchannel()` with **`initialize_mcp = false`** hardcoded:

```rust
let agent = Agent::from_config_with_session_cwd_and_mcp_backchannel(
    &self.config,
    &agent_alias,
    Some(std::path::Path::new(&workspace_dir)),
    false, // ← initialize_mcp is ALWAYS false for ACP
    true,  // exclude_memory
    ...
);
```

This means **ACP sessions never connect to MCP servers**. The `tools_registry` only contains built-in tools.

The comment explains:
> "ACP clients expect `session/new` to return promptly. User-configured MCP servers are external processes/services and can block startup while they time out, so ACP uses this with `initialize_mcp = false`."

But MCP tools are then invisible to the LLM, since they're not in the registry.

## Connection Architecture

```
zerowrapper (web UI)
    │
    ├── Unix socket (JSON-RPC) ──→ zeroclaw daemon (rpc/dispatch.rs)
    │     session/new, session/prompt
    │     initialize_mcp = true (Chat mode, default)
    │
    └── MCP stdio ──→ zw --mcp
          (called BY zeroclaw, not zerowrapper)

zeroclaw daemon
    │
    ├── ACP-over-WebSocket (GET /acp)
    │     initialize_mcp = false (hardcoded)
    │
    ├── ACP stdin/stdout (zeroclaw acp)
    │     initialize_mcp = false (hardcoded)
    │
    └── Unix socket (RPC for TUI/CLI)
          initialize_mcp = true (Chat mode)
```

## RPC Protocol Reference

### `initialize` (per-connection)

Sent once per connection before any other method.

| Param | Type | Default | Effect |
|-------|------|---------|--------|
| `protocol_version` | int | `1` | Must match daemon |
| `tui_id` | string/null | none | Reconnect with previous TUI identity |
| `tui_sig` | string/null | none | HMAC-SHA256 signature for `tui_id` |
| `env` | object | `{}` | Shell env to register for `tui_id` (PATH, keys, etc.) |

**How `tui_id` works** (`rpc/tui_identity.rs`):
- On `initialize`, the TUI sends its shell `env` → daemon stores it in `TuiRegistry`
- On `session/new`, passing `tui_id` makes those env vars available to shell subprocesses
- Not relevant for zerowrapper unless you need shell env forwarding

### `session/new` (per-session)

| Param | Type | Default | Effect |
|-------|------|---------|--------|
| `agent_alias` | string | **required** | Which agent to use |
| `session_id` | string | auto-generated | Unique session UUID |
| `chat_mode` | `"Chat"` or `"Acp"` | `"Chat"` | `"Chat"` → MCP enabled, memory included. `"Acp"` → MCP disabled, memory excluded |
| `cwd` | string/null | none | Working directory for session |
| `exclude_memory` | bool | `false` | Skip memory tools |
| `tui_id` | string/null | none | TUI identity for shell env forwarding |

### `session/prompt` (per-turn)

| Param | Type | Effect |
|-------|------|--------|
| `session_id` | string | Target session |
| `prompt` | string | User message |

## Fixes

### Fix 1: Enable MCP initialization in ACP sessions (code change)

In `zeroclaw/crates/zeroclaw-channels/src/orchestrator/acp_server.rs`, change:

```rust
// Before:
let agent = Agent::from_config_with_session_cwd_and_mcp_backchannel(
    ..., false, // initialize_mcp
    ...
);

// After:
let agent = Agent::from_config_with_session_cwd_and_mcp_backchannel(
    ..., true, // initialize_mcp = true
    ...
);
```

**Downside**: Session creation blocks on MCP server connection (may timeout on slow servers).

### Fix 2: Use deferred MCP loading (config change)

Add to `config.toml`:

```toml
[mcp]
deferred_loading = true
```

With deferred loading, the LLM sees only MCP tool **names** in the system prompt and calls `tool_search` to fetch full schemas on demand. This keeps `session/new` fast while still making MCP tools discoverable.

**Requires**: The LLM must know to call `tool_search`. The built-in `tool_search` tool is always registered.

## MCP Config Reference

All MCP is configured **server-side** in zeroclaw's `config.toml`, NOT controllable from zerowrapper's RPC socket.

### `[mcp]` section

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `true` | Master switch for MCP tool loading |
| `deferred_loading` | bool | `false` | Load tool schemas on-demand via `tool_search` instead of eagerly |

### `deferred_loading` explained

- **`false`** (eager, default): On `session/new`, daemon connects to ALL configured MCP servers, fetches ALL tool schemas, includes them in LLM context. Session creation **blocks** until all MCP servers respond.
- **`true`** (deferred): Session starts instantly. Only MCP tool **names** listed in system prompt. LLM calls `tool_search` RPC to fetch full schema before invoking a deferred tool.

#### Full example (per-agent, no bundles):

```toml
[agents.craw]
risk_profile = "yolo"
runtime_profile = "unbounded"
model_provider = "opencode.opencode_free"

[[agents.craw.mcp_servers]]
name = "zw"
transport = "stdio"
command = "/home/user/zw"
args = ["--mcp"]
env = { KEY = "val" }
tool_timeout_secs = 20
```

#### Bundle example (your current setup):

```toml
[[mcp.servers]]
name = "zw"
command = "/path/to/zw"
args = ["--mcp"]

[mcp_bundles.craw]
servers = ["zw"]

[agents.craw]
mcp_bundles = ["craw"]
```

### `[[agents.<alias>.mcp_servers]]` entries

```toml
[agents.craw]
[[agents.craw.mcp_servers]]
name = "server-name"           # Tool prefix: <name>__<tool>
transport = "stdio"            # stdio | http | sse
command = "/path/to/binary"    # Stdio transport: executable path
args = ["--flag", "val"]       # Command arguments
env = { KEY = "value" }        # Environment variables
url = "https://..."            # HTTP/SSE transport: endpoint URL
headers = { Authorization = "Bearer ..." }  # HTTP headers
tool_timeout_secs = 60         # Per-call timeout
```

## Settings Summary

| Component | Setting | Where | Value |
|-----------|---------|-------|-------|
| zeroclaw daemon | `mcp.enabled` | `config.toml` | `true` (default) |
| zeroclaw daemon | `mcp.deferred_loading` | `config.toml` | `false` (default, eager) |
| zeroclaw daemon | `agents.<name>.mcp_servers` | `config.toml` | Per-agent MCP server definitions |
| zeroclaw daemon | `ZEROCLAW_SOCKET` (env) | env var | Override socket path |
| zeroclaw daemon | data_dir | config / default | `~/.zeroclaw/data` |
| zeroclaw daemon | socket path | derived | `<data_dir>/daemon.sock` or `$ZEROCLAW_SOCKET` |
| zerowrapper | `session/new` `chat_mode` | RPC param | `"Chat"` (default) → MCP enabled |
| zerowrapper | `session/new` `agent_alias` | RPC param | Must match agent name in zeroclaw config |
| zerowrapper | `initialize` `env` | RPC param | Shell env to register (optional) |

## Testing

To verify MCP tools are available in a session:

1. Start daemon: `zeroclaw daemon`
2. Watch logs for: `"MCP: N tool(s) registered from M server(s)"`
3. Check `session/new` response time — if instant, MCP servers may not be connecting

## Key Files

- `zeroclaw/crates/zeroclaw-runtime/src/rpc/dispatch.rs` — RPC session handler (session/new, initialize)
- `zeroclaw/crates/zeroclaw-runtime/src/rpc/types.rs` — RPC param/result structs
- `zeroclaw/crates/zeroclaw-runtime/src/rpc/tui_identity.rs` — TuiRegistry (tui_id lifecycle)
- `zeroclaw/crates/zeroclaw-channels/src/orchestrator/acp_server.rs` — ACP session handler
- `zeroclaw/crates/zeroclaw-runtime/src/agent/agent.rs` — Agent constructor with `initialize_mcp` gate
- `zeroclaw/crates/zeroclaw-config/src/schema.rs` — Config schema (McpConfig, McpServerConfig)
- `zerowrapper/zw-channels/src/zeroclaw_bridge.rs` — zerowrapper's ZeroClawBridge
