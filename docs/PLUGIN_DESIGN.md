# Plugin / Custom Node System — Design Evaluation

Status: **Decision document for P2 #12 (v1.0.0-rc)**
Owner: AgentFlow core
Last updated: 2026-05-08

---

## 1. Goals

Let third-party developers ship `AsyncNode` implementations and (optionally)
agent tools without forking the AgentFlow workspace. Concretely:

1. **Out-of-tree distribution** — a plugin lives in its own repo / cargo
   project / language, builds independently, and is loaded by AgentFlow at
   runtime.
2. **Workflow integration** — once loaded, a plugin's node type works in
   `workflow.yml` exactly like the built-in `llm` / `http` / `template` nodes,
   with full `input_mapping`, `dependencies`, `run_if`, and `FlowValue`
   participation.
3. **Crash isolation** — a panicking or segfaulting plugin must not take down
   the host process; an erroring node must surface as a clean
   `AgentFlowError::NodeExecutionFailed`.
4. **Permission model** — plugins declare required capabilities (filesystem,
   network, shell). Host enforces a deny-by-default policy that mirrors the
   existing `Tool`/`Skill` permission story.
5. **Lifecycle** — `load → register → execute (× N) → unload`, with health
   checks and graceful shutdown.
6. **Observability** — plugin-emitted events appear in the same trace stream
   as built-in node events; `traceparent` propagates across the boundary.

Non-goals for the v1.0.0-rc PoC:

- Marketplace / signature verification (covered by P2 #16).
- Hot-reload during a live `Flow::execute` run.
- Cross-plugin shared state.
- Streaming `AsyncNode` outputs (no AgentFlow node currently streams).

---

## 2. Existing extension surface (what we already have)

The plugin system must compose with, not duplicate, what AgentFlow already
ships:

| Surface | Mechanism | Where it lives |
| --- | --- | --- |
| Built-in DAG nodes | `AsyncNode` impl + `NodeFactory` | `agentflow-nodes/src/factories/` |
| External tools (agent-callable) | MCP stdio server | `agentflow-mcp` |
| Reusable agent capability | Skill (TOML/Markdown manifest) | `agentflow-skills` |
| Custom agent runtime | `AgentRuntime` trait | `agentflow-agents` |

Two consequences:

- **MCP is already a "subprocess plugin" for tools.** Whatever we pick should
  not reimplement MCP for the tool case — the new plugin boundary is for
  *DAG nodes* (and optionally for tools that don't fit MCP's request/response
  shape).
- **`NodeRegistry` and `NodeFactory` (from `agentflow-nodes/src/factory_traits.rs`)
  already provide the in-process registration shape.** A plugin loader's job is
  to produce `NodeFactory` (or `AsyncNode`) instances from out-of-tree binaries
  and add them to that registry.

---

## 3. Three candidate paths

### 3.1 Path A — `dlopen` + `abi_stable`

Ship plugins as native shared libraries (`.so` / `.dylib` / `.dll`). The host
calls `libloading::Library::new(path)` and resolves a fixed C-ABI entry point
that returns a vtable of `extern "C" fn` pointers. `abi_stable` (or `stabby`)
generates the boilerplate and gives Rust types a stable layout across compiler
versions.

**Reference points**: `bevy_dylib`, Lapce plugins (early), `nu` plugins (early),
`nvim-treesitter` parsers.

| Criterion | Assessment |
| --- | --- |
| ABI stability | Fragile. `abi_stable` types are stable, but **every type that crosses the boundary must be `#[sabi]` or `#[repr(C)]`**. `serde_json::Value`, `tokio::Runtime`, `async_trait` futures, `Box<dyn AsyncNode>` — none are. We'd have to mirror them with `RString`, `RHashMap`, `RBoxError`, `RBoxFuture`, etc. |
| Cross-platform | Good (`.so/.dylib/.dll`), but plugin must be rebuilt per (target, libc, glibc-version, macOS-SDK). Distribution = a per-target binary matrix. |
| Security sandbox | **None.** Plugin runs in host address space with full host privileges. A malicious plugin can read every secret in memory, including API keys mid-flight. A buggy plugin can corrupt host heap. |
| Crash isolation | **None.** A `panic` across a non-`abi_stable` boundary is UB; a segfault kills the host. Mitigation (`std::panic::catch_unwind` + always-`abi_stable` types) is achievable but error-prone. |
| Call overhead | Lowest of the three. Function call across a vtable: a few ns. No serialization. |
| Async story | Awkward. `abi_stable::std_types::RBoxFuture` exists but ties the host's async runtime to the plugin's. If plugin links its own `tokio`, both runtimes coexist and deadlock. We'd need to enforce "host owns the runtime; plugin returns `Pin<Box<dyn Future + Send>>` constructed via host-provided `spawn_blocking`-style helpers." |
| Ecosystem | Niche in 2026. `abi_stable` is alive but small; `stabby` is newer. Most plugin systems in similar projects (Lapce, Zed, Helix-tree-sitter) have moved to subprocess or WASM. |
| Polyglot | Rust-only in practice. C plugins possible if they manually mirror the ABI. |
| Dev experience | Requires `cargo build --release` and matching nightly/stable toolchain to host; debugging a crash inside a plugin requires lldb attach. |

**Verdict**: Highest performance, worst safety. Justifiable only for trusted
in-tree extensions where the user controls both sides of the ABI. **Not
recommended** as the primary plugin path for an open-ecosystem v1.0.

---

### 3.2 Path B — WASM (wasmtime / wasmer + Component Model)

Ship plugins as `.wasm` modules using the WebAssembly Component Model + WASI
0.2. The host instantiates each plugin in its own `Store` with explicit imports
(the host functions the plugin can call) and exports (the node functions the
host can call). `wit-bindgen` generates bindings from a `.wit` interface file.

**Reference points**: Spin (Fermyon), Zed extensions, Lapce 2.x plugins,
Envoy-proxy WASM filters, Shopify Functions.

| Criterion | Assessment |
| --- | --- |
| ABI stability | Strong. Component Model + WIT is the stable interface; `wasmtime` rebuilds don't break loaded `.wasm`. WIT is the source of truth, language-agnostic. |
| Cross-platform | Excellent. One `.wasm` runs on macOS/Linux/Windows and amd64/arm64 unchanged. |
| Security sandbox | **Best of three.** Capability-based: plugin sees no host filesystem/network unless explicitly granted via WASI handles. Memory-safe (sandboxed linear memory). CPU/memory limits enforced via fuel + memory caps. |
| Crash isolation | Strong. Trap inside `.wasm` returns `Trap` to host without affecting host process. OOM in plugin returns error, doesn't OOM host. |
| Call overhead | ~µs per call (wasmtime). For DAG nodes that already do LLM/HTTP work (>>10ms), negligible. For pure-compute hot-path nodes, measurable but acceptable. |
| Async story | Good with WASI 0.2 + `wasmtime::component::bindgen!` `async: true` (stable since wasmtime 19.x). Plugin code can `await` host imports (e.g., `host_http_get`); host runs `Store::call_async`. WASI 0.3 brings native async on the WIT side. |
| Ecosystem | Rapidly maturing. Rust (`cargo component`), Go (TinyGo + `wit-bindgen-go`), Python (componentize-py), JS (jco). Spin and Zed are real-world proof. |
| Polyglot | Yes. Plugin can be written in Rust, Go, Python, JS, C/C++ — anything that compiles to a Component. |
| Dev experience | Plugin author writes Rust + `cargo component build`; debugging via `wasmtime --debug` and printf. No native crate access (`reqwest` with native-tls won't work; need `wasi-http` outbound bindings). |
| Dependency cost (host) | `wasmtime` adds ~7 MB binary, ~5–10 s clean build, ~80 transitive crates. `wasmer` similar. We pay this whether plugins are loaded or not (mitigatable via cargo feature). |

**Verdict**: Best long-term answer for an open marketplace. Strong sandbox,
polyglot, deterministic, future-proof via WIT. The cost is the heavyweight host
dependency and the constraint that plugins can't use arbitrary native crates
inside the sandbox (must go through host imports).

---

### 3.3 Path C — Subprocess + JSON-RPC (stdio or socket)

Each plugin is a standalone executable (any language). Host spawns it, talks
JSON-RPC 2.0 over stdio (or a unix socket), exchanging:

- `plugin/initialize` — capability handshake
- `node/list` — declared node types and their schemas
- `node/execute` — invoke a node (request: name + inputs as `FlowValue` JSON;
  response: outputs as `FlowValue` JSON, or error)
- `plugin/shutdown` — graceful exit

**Reference points**: AgentFlow's own MCP integration (`agentflow-mcp`),
Claude Code MCP servers, HashiCorp `go-plugin`, GitHub Copilot LSP-derived
extensions, Terraform providers.

| Criterion | Assessment |
| --- | --- |
| ABI stability | Strong by construction. Wire format = JSON Schema'd JSON-RPC; versioned via `protocol_version` field. Plugin and host can be on different Rust/Go/Python/Node versions. |
| Cross-platform | Excellent. Any OS that can spawn a child process and pipe stdio. |
| Security sandbox | Medium. OS-level process isolation (separate address space, separate FDs). Capability constraints rely on OS sandbox (Apple `sandbox-exec`, Linux seccomp, NT job objects) — we already have this infrastructure in `agentflow-tools/src/sandbox/`. Re-using it for plugins is straightforward. |
| Crash isolation | **Best.** Plugin segfault = `SIGCHLD` we observe; host stays up. Restart policy is straightforward. |
| Call overhead | Highest of the three. JSON encode/decode + pipe IO + context switch: typically 100µs–1ms per call. For DAG nodes with sub-millisecond work this matters; for our actual use cases (LLM, HTTP, file IO) it's noise. |
| Async story | Trivial. Host async-reads stdout, async-writes stdin; plugin's internal model is its own business. |
| Ecosystem | Already proven inside AgentFlow via MCP. Same pattern, slightly different protocol verbs. |
| Polyglot | Yes — any language that can read/write JSON to stdio. |
| Dev experience | Easiest. Plugin author writes a normal program. Debug with `println!` to stderr. No special toolchain. |
| Dependency cost (host) | Minimal — `tokio::process` (already used) + `serde_json` (already used). Zero new crates. |

**Verdict**: Lowest engineering cost, highest crash safety, easiest contributor
experience, reuses existing AgentFlow muscles (MCP transport, OS sandbox, tool
permission policy). The cost is per-call latency that's only relevant for
hot-path compute nodes (which are not what plugins are typically for).

---

## 4. Comparison summary

| Dimension | dlopen+abi_stable | WASM | Subprocess+JSON-RPC |
| --- | --- | --- | --- |
| ABI stability | ⚠️ fragile | ✅ WIT-stable | ✅ wire-stable |
| Cross-platform | ⚠️ per-target binary | ✅ single artifact | ✅ |
| Sandbox | ❌ none | ✅ capability-based | ⚠️ relies on OS sandbox |
| Crash isolation | ❌ shared address space | ✅ trap stays in plugin | ✅ separate process |
| Call overhead | ✅ ns | ⚠️ µs | ⚠️ 100µs–1ms |
| Polyglot | ❌ Rust(+C) only | ✅ Rust/Go/Py/JS/C | ✅ any language |
| Host dep weight | ⚠️ small | ❌ 7 MB + 80 crates | ✅ zero new |
| Existing analog in AF | none | none | **MCP** |
| Async ergonomics | ❌ runtime conflicts | ⚠️ via WASI 0.2 | ✅ trivial |
| Marketplace fit | ❌ binary matrix | ✅ single artifact | ✅ executable per OS |
| Dev experience | ❌ toolchain match | ⚠️ `cargo component` | ✅ `cargo build` |

---

## 5. Decision

**Adopt subprocess + JSON-RPC as the primary plugin runtime for v1.0.0-rc.**
Plan WASM as a parallel runtime in v1.1+ once we have a real performance
motivation and a WIT contract proven in the wild. Reject `dlopen+abi_stable`
for the open ecosystem; revisit only if a trusted internal use case demands ns
latency.

Rationale:

1. **Latency budget**: AgentFlow node execution is dominated by LLM / HTTP /
   file IO (≥10 ms). A 200µs JSON-RPC round-trip is <2 % overhead. For the few
   compute-hot nodes (e.g., embedding postprocessing), built-in nodes remain
   the right answer.
2. **Reuse**: `agentflow-mcp` already implements stdio JSON-RPC with
   reconnect / timeout / latency benchmarks, and `agentflow-tools/src/sandbox/`
   already implements macOS `sandbox-exec` and Linux seccomp wrappers. The
   plugin host is mostly composition of existing crates plus a new protocol.
3. **Crash safety beats µs**: A plugin author writing a buggy node is the
   common failure mode. OS-level isolation makes "plugin panicked, host
   continues" the default behavior, not a feature we have to engineer.
4. **Polyglot ecosystem**: Plugin authors in Python or Go shouldn't need to
   learn Rust ABI rules. Subprocess opens the door immediately; WASM opens it
   later with more effort per language.
5. **Marketplace shipping**: Distributing one executable per (linux-amd64,
   linux-arm64, darwin-amd64, darwin-arm64, windows-amd64) is unfortunate but
   solved (Goreleaser, cargo-dist). Distributing native dylibs adds a
   glibc-version axis that we don't want to pay for at v1.0.
6. **WASM is the right v1.1+ answer for the *in-process* tier**: For nodes
   that need <10µs overhead and run inside the sandbox, WASM wins. We document
   this as a planned second runtime; `Plugin` manifest already has a `runtime`
   field so this is a forward-compatible decision.

---

## 6. Architecture for the chosen path

### 6.1 Components

```
agentflow-core/src/plugin/
├── mod.rs              -- public surface
├── manifest.rs         -- `plugin.toml` parsing
├── protocol.rs         -- JSON-RPC request/response types
├── host.rs             -- PluginHost: spawn / handshake / dispatch
├── node.rs             -- PluginNode: AsyncNode adapter
└── registry.rs         -- PluginRegistry: name -> PluginHost handle
```

```
agentflow-cli/src/commands/plugin/  (follow-up; not in PoC scope)
├── mod.rs
├── install.rs   -- copy plugin dir into ~/.agentflow/plugins/
├── list.rs      -- list installed plugins + declared nodes
├── inspect.rs   -- print manifest + capabilities
└── uninstall.rs
```

### 6.2 `plugin.toml` manifest schema

```toml
# Required
[plugin]
name        = "agentflow-plugin-image-ocr"
version     = "0.1.0"
runtime     = "subprocess"        # "subprocess" (v1.0) | "wasm" (v1.1+)
entrypoint  = "bin/ocr-plugin"    # path relative to manifest, or "$PATH" name
protocol    = "agentflow.plugin/1"  # protocol version

# Required: enumerate node types this plugin contributes
[[plugin.nodes]]
type        = "image_ocr"
description = "Extract text from images using local Tesseract"
inputs      = { image = "FlowValue::File", language = "string?" }
outputs     = { text = "string", confidence = "number" }

# Required: capability declarations (deny by default)
[plugin.capabilities]
filesystem  = ["read:./models", "read:$INPUT_FILE"]
network     = []                  # empty = no outbound network
processes   = []
env_vars    = ["TESSDATA_PREFIX"]

# Optional: signature (P2 #16 territory; wired but not enforced in v1.0-rc)
[plugin.signature]
algorithm   = "ed25519"
public_key  = "..."
signature   = "..."

# Optional (P3.4-PR.1): dry-run smoke invocation. When present, lets
# `agentflow doctor` verify the entrypoint binary at least starts
# cleanly without speaking the JSON-RPC protocol. Plugins are
# expected to honor a fast, side-effect-free invocation that exits
# `expected_exit` (default 0) well within the timeout.
#
#   `args`           — required, non-empty CLI argv passed to the entrypoint.
#                      Typical values: `["--smoke"]`, `["--version"]`.
#   `timeout_ms`     — optional, default 1000. Wall-clock cap; anything
#                      past ~5s is almost certainly a hang.
#   `expected_exit`  — optional, default 0. Allows plugins that exit
#                      `64` (usage) or similar as their dry-run success.
#
# Absent / commented out ⇒ doctor skips the smoke for this plugin.
[plugin.dry_run]
args            = ["--smoke"]
timeout_ms      = 1000
expected_exit   = 0
```

Why TOML, not YAML: matches `skill.toml` and `Cargo.toml` precedent in the
workspace; `agentflow-skills` already builds against TOML manifests.

### 6.3 JSON-RPC protocol

Wire format: `Content-Length: N\r\n\r\n{json}` (LSP-style framing) on
stdin/stdout. Stderr is plugin's free-form log channel (host forwards to
`tracing::debug!(target = "plugin::{name}")`).

#### Methods (host → plugin)

```
plugin/initialize
  params: { host_version, protocol_version, capabilities_granted }
  result: { plugin_name, plugin_version, nodes: [NodeSpec] }

node/execute
  params: { node_type, inputs: { [k]: FlowValue }, run_id, span_context? }
  result: { outputs: { [k]: FlowValue } }
  error : { code, message, data? }

plugin/shutdown
  params: {}
  result: {}
```

#### Notifications (plugin → host)

```
plugin/log
  params: { level: "trace"|"debug"|"info"|"warn"|"error", message, fields? }

plugin/event
  params: { run_id, kind: "NodeProgress" | ..., data }
```

`FlowValue` is serialized exactly as it already is on the workflow checkpoint
side (`{"type": "json"|"file"|"url", ...}`), so plugin authors and host share
the existing schema in `agentflow-core::value`.

### 6.4 Lifecycle

```
PluginHost::load(manifest_path)
  -> read+validate manifest
  -> apply OS sandbox profile from `[plugin.capabilities]`
  -> tokio::process::Command::spawn child
  -> JSON-RPC `plugin/initialize` (timeout = 10 s)
  -> register declared nodes into NodeRegistry as PluginNode shims

Flow execution
  -> NodeRegistry::create_node("image_ocr", ...)
  -> returns PluginNode { plugin_id, node_type }
  -> Flow::execute calls AsyncNode::execute
  -> PluginNode forwards to PluginHost::call("node/execute", ...)
  -> wait for response (per-call timeout = node spec or default 60 s)

PluginHost::shutdown
  -> JSON-RPC `plugin/shutdown` (timeout = 5 s)
  -> if no exit within 10 s, SIGTERM → SIGKILL
```

Crash recovery: if the plugin process dies mid-call, the in-flight
`node/execute` returns `AgentFlowError::AsyncExecutionError { message: "plugin
'<name>' exited unexpectedly: <reason>" }`. The host marks the plugin
"unavailable"; subsequent calls fail fast. A `--restart-on-crash` policy can
respawn for `Idempotent` workflows but is opt-in.

### 6.5 Permission model

Bridges to `agentflow-tools::sandbox` via the
`agentflow_core::plugin::CommandPreparer` trait — `agentflow-core` itself
takes no dependency on the platform sandbox crates, so an embedded host
can opt into enforcement (or not) without dragging in libseccomp / a
sandbox-exec profile generator. The CLI ships
`OsSandboxPluginPreparer` (in `agentflow-cli/src/executor/plugin.rs`),
which is an adapter that reads a plugin's `[plugin.capabilities]` block,
calls `agentflow_tools::sandbox::default_backend()`, and runs the
backend's `wrap_command` against the spawned subprocess.

#### Translation rules

`[plugin.capabilities]` → `(Vec<Capability>, SandboxScope)`:

| Manifest entry              | Capability granted | Scope effect                                                                          |
| --------------------------- | ------------------ | ------------------------------------------------------------------------------------- |
| `filesystem = ["read:X"]`   | `Capability::FsRead`  | `X` added to `scope.read_paths` (relative resolved against the manifest dir).      |
| `filesystem = ["write:X"]`  | `Capability::FsWrite` | `X` added to **both** `read_paths` and `write_paths` (writable implies readable). |
| `filesystem = ["X"]` (bare) | `Capability::FsRead`  | Same as `read:X`.                                                                  |
| `network` non-empty         | `Capability::Net`     | No path effect.                                                                    |
| `processes` non-empty       | `Capability::Exec`    | No path effect.                                                                    |
| `env_vars` non-empty        | `Capability::Env`     | No OS-level effect (recorded for audit only — neither macOS sandbox-exec nor Linux seccomp can scrub the env passed to `execve`). |

Two permissive defaults match
`agentflow_tools::builtin::shell::build_scope_from_policy`:

* The manifest directory is **always** added to `scope.read_paths` so the
  plugin executable can be resolved and exec'd.
* If `filesystem` is empty, `/tmp` is also added to `scope.read_paths` so
  trivial plugins still have a working temp space.

`scope.working_directory` is set to the manifest directory.

Filesystem entries that fail to parse (e.g. `read:`) surface as
`PluginError::PreparerRejected`; the plugin never spawns.

#### Opt-in

The CLI keeps the v0.3 PoC behaviour (no OS sandbox) by default. Set
`AGENTFLOW_PLUGIN_SANDBOX=1` (any non-empty, non-`0` value) before
`agentflow workflow run` to wrap plugin spawns in the platform backend:

```bash
AGENTFLOW_PLUGIN_SANDBOX=1 agentflow workflow run plugin_workflow.yml
```

On macOS this materialises a `sandbox-exec` profile derived from the
capability set; on Linux it installs a seccomp BPF filter through
`Command::pre_exec`. On other platforms the noop backend is used and the
host logs that enforcement is unavailable.

#### Embedding directly

A library caller (not via CLI) attaches an arbitrary `CommandPreparer`
through the new builder:

```rust
use std::sync::Arc;
use agentflow_core::plugin::PluginHost;

let host = PluginHost::builder()
  .with_command_preparer(Arc::new(my_preparer))
  .load("./plugin.toml")
  .await?;
```

This is the seam the CLI uses; it is also the seam custom hosts and
tests use to record / reject preparer invocations without touching the
real OS sandbox (see `agentflow-core/tests/plugin_poc.rs`).

#### Workflow-level allowlist

The host enforces an additional **plugin allowlist** at the workflow level:
`workflow.yml` must explicitly list `plugins: [name@version]` before its node
types resolve, mirroring the Skill / MCP server allowlist. This prevents a
workflow author from being surprised by a plugin transitively pulled in by a
Skill. (Allowlist enforcement is tracked separately and is not part of the
CommandPreparer wiring.)

### 6.6 Observability

- Plugin logs (stderr or `plugin/log` notifications) → forwarded to `tracing`
  with `target = "plugin::{name}"`, automatically picked up by
  `agentflow-tracing`.
- `traceparent` from the active span is passed in `node/execute.params.span_context`.
  Plugin authors who care about distributed tracing can attach it to their own
  HTTP calls (host provides a small helper crate for Rust plugins).
- `WorkflowEvent::NodeStarted` / `NodeCompleted` are emitted by the host
  around each `node/execute` call — plugins don't need to know about them.

### 6.7 Forward path to WASM (v1.1+)

The `runtime` field in `plugin.toml` makes this a clean swap:

- `runtime = "subprocess"` → today's `PluginHost` (v1.0).
- `runtime = "wasm"` → v1.1's `WasmPluginHost` (wraps `wasmtime::component::Linker`,
  same `node/execute` semantics, same `FlowValue` serialization).

Both runtimes implement a `PluginRuntime` trait so `Flow` and `NodeRegistry`
don't care which is which. This lets us ship subprocess in v1.0, validate the
manifest format and protocol surface, then add WASM without an SDK breakage.

---

## 7. Open questions for follow-ups

| # | Question | Notes |
| --- | --- | --- |
| 1 | Plugin → tool registration | A plugin should also be able to declare `[[plugin.tools]]` and have them flow into `ToolRegistry`. Out of PoC scope; design slot reserved in manifest. |
| 2 | Streaming outputs | Some node types (LLM-like) want to stream tokens. JSON-RPC doesn't natively stream. Path: `node/execute_streaming` returns a notification stream `node/output_chunk` until `node/output_done`. Defer until a streaming built-in node exists. |
| 3 | Hot reload | Plugin SIGHUP → re-read manifest, restart child. Useful for plugin authors; not v1.0 critical. |
| 4 | Marketplace + signatures | Owned by P2 #16. Manifest already has `[plugin.signature]` slot. |
| 5 | Plugin-to-plugin calls | Forbidden in v1.0. All inter-plugin coordination goes through workflow DAG edges. |

---

## 8. PoC scope

The accompanying PoC demonstrates the **smallest end-to-end loop** that proves
the chosen path is viable:

- `agentflow-core/src/plugin/` — manifest, JSON-RPC protocol, host, node
  adapter, registry.
- `agentflow-core/examples/plugins/echo_plugin/` — standalone cargo binary
  that registers one node type (`echo_uppercase`).
- `agentflow-core/examples/plugin_host_demo.rs` — host-side demo: load
  manifest, register, execute, assert result.
- Integration test: spawn the example plugin, run `node/execute`, verify
  output, verify clean shutdown, verify crash isolation (kill plugin
  mid-call → host returns error, host still runnable).

Out of PoC scope (deferred to follow-up tasks within #12):

- `agentflow plugin install/list/inspect/uninstall` CLI.
- OS sandbox enforcement (the `SandboxedCommand` integration is mechanical
  but adds platform branches; first prove the protocol).
- Signature verification.
- WASM runtime.

## 9. Workflow YAML integration

Once the host is in place, plugin nodes are addressable from `workflow.yml`
via the dedicated `plugin` node type. The CLI exposes this behind the
`plugin` cargo feature on `agentflow-cli` so the default build stays free
of the subprocess runtime.

```yaml
name: Plugin Workflow
nodes:
  - id: shout
    type: plugin
    parameters:
      manifest: ./plugins/echo/plugin.toml   # path to plugin.toml
      node_type: echo_uppercase              # type declared by the plugin
      text: "hello plugin"                   # forwarded as input
```

Resolution rules:

- `manifest` is required; relative paths resolve against the current working
  directory of the `agentflow workflow run` invocation.
- `node_type` is required and must match one of the `[[plugin.nodes]]` types
  declared in the manifest.
- All other `parameters` keys (other than `manifest` and `node_type`) become
  `initial_inputs` for the plugin call. `input_mapping` works the same way as
  for built-in nodes.

Lifecycle inside the CLI:

- The first workflow node referencing a given manifest path causes the host
  to spawn the plugin subprocess and run the `plugin/initialize` handshake.
- Subsequent nodes that point at the same manifest path reuse the same
  process via a per-run `Mutex<HashMap<PathBuf, Arc<PluginHost>>>` cache, so
  spawn cost is paid once per `workflow run`.
- The cache is process-wide; when the CLI process exits, child plugins are
  cleaned up by `kill_on_drop(true)` on the underlying `tokio::process::Child`.

Build and run:

```bash
cargo build -p agentflow-core --features plugin --bin agentflow-echo-plugin
cargo run  -p agentflow-cli  --features plugin -- workflow run plugin_workflow.yml
```

Failure modes surface as `AgentFlowError`:

- Missing/invalid manifest path → `NodeInputError` at execute time.
- Manifest parse / protocol mismatch / handshake failure →
  `AsyncExecutionError` carrying the underlying `PluginError`.
- Unknown plugin-declared node type → `RemoteError` from the plugin's own
  protocol handler (surfaced as `AsyncExecutionError`).

## 10. CLI reference

`agentflow plugin` ships behind the `plugin` cargo feature on
`agentflow-cli`. It manages plugins on disk only — none of the four verbs
spawn the plugin subprocess. The `workflow run` path (§9) is the only thing
that talks JSON-RPC to plugins.

The default plugins root is `~/.agentflow/plugins/`. Each verb accepts
`--dir <path>` to override it (used by tests and by users with
non-standard layouts).

### `agentflow plugin install <source-dir> [--dir <plugins-dir>] [--force]`

Validates the manifest at `<source-dir>/plugin.toml`, then copies the
whole source tree into `<plugins-dir>/<name>/`. Refuses to copy if the
target already exists unless `--force` is set, and refuses to install
into the source's own subtree (defense-in-depth against `--dir` typos).
Executable bits on Unix are preserved. Warns (does not fail) if the
manifest's declared entrypoint is missing in the source — that is
common for plugins that build the entrypoint via `cargo build` in a
separate step.

### `agentflow plugin list [--dir <plugins-dir>]`

Scans every direct subdirectory of `<plugins-dir>` that contains a
`plugin.toml`. For each one prints `name@version`, runtime, the
resolved entrypoint and whether it exists, the declared node types,
and a one-line capability summary (`fs:N net:N proc:N env:N`).
Invalid manifests are surfaced as `❌ <path> — <error>` so a broken
install doesn't hide the rest.

### `agentflow plugin inspect <plugin-dir-or-manifest>`

Accepts either a plugin directory or a `plugin.toml` path directly.
Prints the full manifest in human form: name / version / runtime /
protocol / resolved absolute entrypoint / `exists` / `executable`
status, every declared node, and the four capability lists. Reports
`Status: valid` or `Status: invalid — <reason>` at the end. This
command never spawns the plugin; use it to diagnose a misbehaving
install before reaching for `workflow run`.

### `agentflow plugin uninstall <name> [--dir <plugins-dir>] [--force]`

Removes `<plugins-dir>/<name>/`. Refuses to remove a directory that
does not contain a `plugin.toml` (so a typoed `<name>` cannot wipe an
unrelated tree). With `--force` the command is idempotent: missing
plugins succeed silently rather than erroring.

### Examples

```bash
# Build the in-tree reference plugin and install it under the default
# plugins root (~/.agentflow/plugins/).
cargo build -p agentflow-core --features plugin --bin agentflow-echo-plugin
mkdir -p ./echo-plugin/bin
cp target/debug/agentflow-echo-plugin ./echo-plugin/bin/echo-plugin
cat > ./echo-plugin/plugin.toml <<'TOML'
[plugin]
name = "echo-plugin"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/echo-plugin"

[[plugin.nodes]]
type = "echo_uppercase"
description = "Uppercase a JSON string."
TOML

agentflow plugin install ./echo-plugin
agentflow plugin list
agentflow plugin inspect ~/.agentflow/plugins/echo-plugin
agentflow plugin uninstall echo-plugin
```
