# ZeroClaw Patches

These are the changes needed after downloading fresh zeroclaw source from GitHub.
Patch them, then build.

## Change 1: Enable MCP + Memory in ACP sessions

Required only if using ACP protocol (stdin/stdout or WebSocket at `/acp`).
**Not needed for RPC socket users** (zerowrapper, TUI).



**File**: `zeroclaw/crates/zeroclaw-channels/src/orchestrator/acp_server.rs`

**Change**: In all 3 `Agent::from_config_with_session_cwd_and_mcp_backchannel()` calls, swap the two boolean parameters:

```rust
// BEFORE (all 3 locations — lines 580, 793, 995):
            false,   // initialize_mcp
            true,    // exclude_memory
            self.sop_engine.clone(),
            self.sop_audit.clone(),
            self.canvas_store.clone(),

// AFTER:
            true,    // initialize_mcp  ← MCP servers start and tools load
            false,   // exclude_memory  ← long-term memory is available
            self.sop_engine.clone(),
            self.sop_audit.clone(),
            self.canvas_store.clone(),
```

**3 locations to change** — search for `from_config_with_session_cwd_and_mcp_backchannel` to find them all.

## Effect

| Feature | Before (default zeroclaw) | After (patched) |
|---------|--------------------------|-----------------|
| MCP tools in ACP/daemon sessions | ❌ Not loaded (`initialize_mcp = false`) | ✅ Loaded and discoverable |
| Long-term memory in ACP sessions | ❌ Excluded (`exclude_memory = true`) | ✅ Available |
| Session creation speed | Fast (no MCP blocking) | Slower (waits for MCP servers) |
| Other behavior | Unchanged | Unchanged |

## Change 2: Debug logging for MCP tool visibility

Added logging to trace why MCP tools registered in `tools_registry` might not reach the LLM.

### 2a. `loop_.rs` — `filter_tool_specs_for_turn`

**File**: `zeroclaw/crates/zeroclaw-runtime/src/agent/loop_.rs`

Added `record!` at end of `filter_tool_specs_for_turn()` to log how many tools were removed by tool filter groups. Uses `Action::Note`.

**Search**: `pub fn filter_tool_specs_for_turn`

### 2b. `loop_.rs` — `register_eager_mcp_tool_if_allowed`

**File**: `zeroclaw/crates/zeroclaw-runtime/src/agent/loop_.rs`

Added `record!` with `Action::Skip` when an MCP tool is blocked by tool access policy.

**Search**: `pub fn register_eager_mcp_tool_if_allowed`

### 2c. `tool_specs.rs` — `build_iteration_tool_specs`

**File**: `zeroclaw/crates/zeroclaw-runtime/src/agent/turn/tool_specs.rs`

Added `record!` with `Action::Skip` when a tool is excluded by `excluded_tools` list.

**Search**: `.filter(|tool|` near `build_iteration_tool_specs`

### 2d. `turn/mod.rs` — tool spec assembly

**File**: `zeroclaw/crates/zeroclaw-runtime/src/agent/turn/mod.rs`

Added `record!` with `Action::Note` before `request_tools` assignment listing all tool names and MCP tool count.

**Search**: `let request_tools = if use_native_tools`

## Build

```bash
cd zeroclaw/
cargo build --release
```

The binary will be at `target/release/zeroclaw`.
