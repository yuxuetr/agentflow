# WASM Plugin Runtime — Evaluation 1-Pager

Status: **Decision document for P10.19.1**
Owner: AgentFlow core
Last updated: 2026-05-20
Closes: P10.19.1 (HIGH — pre-GA)

This 1-pager is the focused follow-up to `docs/PLUGIN_DESIGN.md`
(2026-05-08), which generically picked subprocess + JSON-RPC for
v1 and listed "WASM" as the v1.1+ candidate without committing
to a specific runtime. P10.19.1 asks the narrower question:
**among `wasmtime`, `wasmer`, and `extism`, which is the right
WASM runtime, and should we invest in it before v1.0 GA or push
to v2.0?**

The recommendation up front: **push to v2.0.** Reasoning below.

---

## 1. What we have today

The subprocess plugin runtime is the only one shipped. Surface
that any WASM runtime would have to match or strictly extend:

- `agentflow-core/src/plugin/manifest.rs`: `PluginManifest`
  parses `plugin.toml`. `PluginRuntime::{Subprocess, Wasm}` is
  already an enum variant — `Wasm` parses today but errors at
  load time with `ManifestError::UnsupportedRuntime`. This means
  the manifest schema is already forward-compatible; the work
  to add WASM is implementing the host side, not changing the
  manifest format.
- `agentflow-core/src/plugin/host.rs`: `PluginHost` spawns the
  subprocess, handshakes via `plugin/initialize`, dispatches
  `node/execute`, and shuts down via `plugin/shutdown`. It
  exposes `execute_node(node_type, inputs: HashMap<String,
  FlowValue>) -> Result<HashMap<String, FlowValue>>` to the rest
  of the workspace.
- `agentflow-core/src/plugin/node.rs`: `PluginNode` is the
  `AsyncNode` adapter the registry hands back; the rest of
  `Flow` doesn't know it's talking to a plugin.
- `agentflow-core/src/plugin/protocol.rs`: 4 host→plugin methods
  (`plugin/initialize`, `node/execute`, `plugin/shutdown`,
  `plugin/log` notification), all carrying `FlowValue` payloads
  serialized via serde.

A WASM runtime that wants to slot in here exposes the same
public surface (`PluginHost::load(manifest_path) -> Self`,
`execute_node(...)`, `shutdown(...)`). Everything above
`PluginNode` is runtime-agnostic.

---

## 2. The three candidate WASM runtimes

### 2.1 `wasmtime` (Bytecode Alliance)

Reference implementation of the WebAssembly Component Model and
WASI 0.2 (stable). Maintained by Bytecode Alliance + Fastly +
Microsoft. Shipped in Spin, Envoy, Fastly Compute@Edge.

- **API**: `Engine` (compile cache) → `Store` (per-instance
  isolation, fuel/memory caps) → `Linker` (host imports) →
  `Instance` (loaded module). Component Model surface via
  `wasmtime::component::bindgen!` from a `.wit` file.
- **Async**: `Store::call_async` + `bindgen! { async: true }`
  has been stable since wasmtime 19.x; current line is 25.x
  (as of 2026-05). Plugin code can `await` host imports, the
  host runs the future on its own tokio runtime. This matches
  our existing `AsyncNode::execute` async signature without
  contortion.
- **Sandbox**: Capability-based. Plugin sees zero host
  filesystem/network unless we explicitly wire a `WasiCtx` with
  `preopened_dir(...)` / `inherit_network()`. Fuel-based CPU
  cap (`Store::add_fuel`); memory cap via `Store::limiter`.
- **Binary cost**: ~7 MB added to host binary (cargo-bloat
  measurement from a comparable Bytecode Alliance consumer);
  ~80 transitive crates; ~10 s clean release-mode build hit.
- **Ecosystem**: Plugin authors use `cargo component build`
  (Rust), `tinygo build -target=wasi-0.2` (Go),
  `componentize-py` (Python), `jco` (JS). Polyglot is real and
  shipping.
- **Verdict**: The default WASM runtime for serious production
  use in 2026. If we adopt WASM at all, this is the runtime.

### 2.2 `wasmer`

Independent runtime maintained by Wasmer Inc. Targets the same
core spec; was historically more permissive about non-standard
extensions (Emscripten ABI, Wasmer-specific WASIX async). Has
its own Component Model story but trails `wasmtime` on stable
release cadence.

- **API**: `Store` → `Module` → `Instance` with similar shape;
  imports/exports via macros or builder.
- **Async**: Available via `wasmer-wasix` (Wasmer's superset of
  WASI), but the upstream-Component-Model async path is less
  established than wasmtime's.
- **Sandbox**: Equivalent isolation model; capability surface
  feels similar but the Wasmer-specific WASIX extends WASI 0.2
  in ways that aren't portable to wasmtime/Spin/Envoy plugins.
  Adopting Wasmer means the AgentFlow plugin ecosystem
  bifurcates from the broader Bytecode Alliance ecosystem.
- **Binary cost**: Similar magnitude (~6-8 MB).
- **Ecosystem**: Smaller. The 2024-2026 trend across reference
  consumers (Spin, Zed, Lapce 2.x, Shopify Functions, Fastly,
  Cloudflare-on-component-model) has been to standardize on
  wasmtime + WIT + WASI 0.2.
- **Verdict**: Functional but the wrong bet for an *open*
  marketplace. Plugins compiled against Wasmer-specific WASIX
  wouldn't run unmodified on wasmtime, and the broader tooling
  (`cargo component`, `wit-bindgen`, `jco`) is wasmtime-aligned.
  Eliminating.

### 2.3 `extism`

Higher-level "plugin framework" built on top of either wasmtime
or wasmer (selectable backend). Provides a simpler host-import
model: instead of WIT-defined imports/exports, plugins call
`extism_pdk::host_fn!` and the host registers Rust closures
keyed by name. Plugin authors use `extism_pdk` (Rust), the JS
SDK, the Python SDK, etc.

- **API**: `extism::Plugin::new(wasm_bytes, host_functions,
  /*with_wasi=*/ true)` → `plugin.call::<&str, &[u8]>("fn",
  input)`. Inputs/outputs are bytes, not typed via WIT.
- **Async**: Plugin-side calls are synchronous; the SDK doesn't
  expose host imports as async-from-the-plugin-side. Host-side
  `plugin.call()` can be wrapped in `tokio::task::spawn_blocking`
  but that breaks the "host imports can be awaited from plugin
  code" model that wasmtime+WIT gives us.
- **Sandbox**: Inherits the backend's (wasmtime by default).
  Capability granting is via the host-fn registration list —
  simpler than a full WIT contract but less expressive (no typed
  resource handles, no streaming).
- **Binary cost**: Same as wasmtime (extism vends wasmtime
  underneath) + ~1 MB extism shim.
- **Ecosystem**: Niche-popular for "scripts inside your app"
  use cases (Dylibso's marketing target: extending products
  with sandboxed user code). Less aligned with our use case
  (third-party DAG nodes that participate in `FlowValue` typing
  and need async access to host imports like HTTP fetch).
- **Verdict**: Wrong abstraction tier for AgentFlow's plugin
  story. extism optimises for "scripts that compute against
  bytes"; AgentFlow plugins are "typed nodes that participate
  in a typed dataflow graph." We'd fight the framework. If we
  adopt WASM, we want WIT + Component Model directly, not the
  extism abstraction.

---

## 3. Comparison matrix

| Dimension | wasmtime | wasmer | extism |
| --- | --- | --- | --- |
| Component Model + WIT (typed boundary) | ✅ first-class | ⚠️ trailing | ❌ bytes-only |
| WASI 0.2 stable async | ✅ since 19.x | ⚠️ via WASIX | ❌ sync calls only |
| Ecosystem alignment in 2026 | ✅ industry default | ⚠️ bifurcating | ⚠️ scripts-in-app niche |
| Polyglot author SDK | Rust/Go/Py/JS via `cargo component` etc. | Rust/Go/Py via Wasmer SDKs | Rust/Go/Py/JS via `extism_pdk` |
| Sandbox primitives | capability + fuel + memory cap | equivalent | inherits backend |
| Host async imports from plugin | ✅ | ⚠️ | ❌ |
| Host binary cost | ~7 MB / ~80 crates | ~6-8 MB | ~7 MB + 1 MB shim |
| Adoption proof in 2026 | Spin, Envoy, Fastly, Zed | smaller | Dylibso products |
| AgentFlow fit | ✅ if we adopt WASM | ❌ ecosystem split | ❌ abstraction mismatch |

If we ever adopt WASM, the answer is **wasmtime + WIT +
WASI 0.2**. The remaining question is *when*.

---

## 4. Should we ship WASM before v1.0 GA?

### 4.1 What WASM would buy us that subprocess doesn't

1. **Single-binary plugin distribution.** One `.wasm` runs on
   macOS/Linux/Windows, amd64/arm64. Subprocess plugins need a
   per-target build matrix (handled by Goreleaser / cargo-dist
   today, but each plugin author has to set that up).
2. **Sub-millisecond cold start.** `Engine::precompile_module`
   + `Module::deserialize_file` lets a wasmtime host start a
   plugin instance in ~100µs. A subprocess plugin needs a
   `tokio::process::Command::spawn` + `plugin/initialize`
   handshake; typical cold start is 50-200 ms. Matters for
   short-lived workflows or per-request plugin instantiation.
3. **Built-in capability sandbox.** Subprocess plugins rely on
   `agentflow-tools/src/sandbox/` (macOS `sandbox-exec` /
   Linux seccomp) for OS-level isolation. wasmtime's
   capability model is finer-grained (per-resource handles)
   and portable across host OSes.
4. **Memory and CPU caps.** `Store::limiter` + `Store::add_fuel`
   make resource limits trivial; subprocess plugins need
   `setrlimit`-style enforcement which we don't currently
   plumb through the sandbox layer.

### 4.2 What WASM would cost us pre-GA

1. **+7 MB on every `agentflow` binary even when no plugins
   are loaded.** Mitigation: feature-gate `wasm` so the cost
   is opt-in. But feature-gating splits the test matrix and
   adds CI surface.
2. **A `.wit` contract is a forward-compatibility freeze.**
   Once we ship `agentflow.plugin/wasm.v1.wit`, every change
   has to maintain WIT backward compatibility. Doing this
   *before* we know the right ergonomics is premature.
3. **Polyglot SDKs need polyglot examples + docs + CI smoke.**
   The subprocess runtime has one example (`agentflow-core/
   examples/plugin_host_demo.rs`) plus the `examples/
   ecosystem/plugins/` set; a WASM runtime worth shipping
   needs at least Rust + TinyGo + componentize-py examples
   so the polyglot promise is real. That's ~3 person-weeks of
   example + CI work alone.
4. **Async host imports require a `bindgen!` async story.**
   Stable since wasmtime 19.x, but our existing `AsyncNode`
   has multi-output `HashMap<String, FlowValue>` returns. A
   WIT analog needs `resource` types or careful record
   modelling; the right WIT shape isn't obvious until we have
   2-3 real WASM plugins to design against.
5. **Sandbox already exists for subprocess.** The `os_sandbox`
   backends (sandbox-exec / seccomp) shipped in N9 cover the
   "untrusted plugin" threat model adequately for v1.0. WASM
   is a *better* answer, not a *missing* answer.

### 4.3 Pre-GA opportunity cost

The remaining HIGH pre-GA work (P10.0.x) is operator-facing:
production-deployment dress rehearsal, `cargo publish --dry-run`,
v1.0.0-rc.1 tag, fresh-VM doctor smoke. None of those benefit
from WASM. The remaining v1.x-tier work (P10.14.1 retention
override, P10.15.1 backup CLI, P10.16.1 worker JWT) is
operator-facing too.

Shipping WASM pre-GA means delaying GA by ~6-8 person-weeks
(WIT design + wasmtime integration + 3 polyglot examples +
docs + tests) on a feature whose primary win — sub-ms cold
start — does not solve a complaint anyone has filed against
the subprocess runtime. The subprocess cold-start of 50-200 ms
is dominated by the first LLM call's TCP handshake in any
realistic workflow.

---

## 5. Decision

**Push WASM to v2.0. Re-evaluate when at least one of these
conditions holds:**

1. **Concrete latency complaint.** A real user (or our own
   workloads) hits a workflow where the 50-200 ms subprocess
   cold-start is the bottleneck. Until then it's noise next to
   LLM RTT.
2. **Polyglot plugin demand.** A non-Rust contributor wants to
   ship an AgentFlow plugin and the subprocess polyglot story
   (write a binary in any language) isn't enough for them. We
   haven't seen this request.
3. **Single-binary distribution complaint.** Plugin authors
   start asking us to host a multi-platform build matrix on
   their behalf, or marketplace consumers complain about per-
   platform downloads. The plugin marketplace shipped in N10
   already handles per-platform artefacts; we can revisit when
   the artefact set proves cumbersome at scale.
4. **Third-party precedent forcing function.** A peer project
   (Helix, Zed, Lapce) ships a WASM-Component-Model plugin
   ecosystem that our users are already familiar with, and the
   ergonomics gap becomes a competitive disadvantage.

Until at least one condition holds, the WASM investment is
"better story for hypothetical future users" — not a fix for
any current friction.

The `PluginRuntime::Wasm` enum variant stays in `manifest.rs`
as a forward-compatible reservation. Today it errors with
`UnsupportedRuntime` at load; v2.0 would replace that error
with a real `WasmPluginHost` keeping the same outer
`PluginHost` surface.

---

## 6. What v2.0 work would look like (sketch, not commitment)

If we re-cross the threshold in v2:

1. **WIT contract** (`agentflow-core/wit/plugin.v1.wit`):
   define `node`, `flow-value`, `execute(node, inputs) ->
   (outputs, error)` with at least the same expressivity as
   the current JSON-RPC `ExecuteParams` / `ExecuteResult`.
2. **`WasmPluginHost` in `agentflow-core/src/plugin/wasm.rs`**:
   load the `.wasm` via `Engine + Component::from_file`,
   wire host imports for `host_log`, `host_http_get` (gated
   by `Capabilities::network`), `host_fs_read` (gated by
   `filesystem`). Same `execute_node` outer signature.
3. **Plugin manifest extension**: extend `PluginSection` with
   a `wasm_caps` sub-table only when `runtime = "wasm"`, so
   the existing subprocess capabilities translate to WASI
   preopens.
4. **Polyglot example matrix**: one example each for Rust
   (`cargo component`), Python (`componentize-py`), Go
   (TinyGo + `wit-bindgen-go`). All three must round-trip
   through `Flow::execute` in CI.
5. **Feature flag `agentflow-core/Cargo.toml [features] wasm
   = ["wasmtime", "wasmtime-wasi", ...]`** — opt-in so the
   default release binary stays small.

Estimated scope: ~6-8 person-weeks for someone familiar with
both AgentFlow's plugin host and the Component Model. Cargo
features keep this additive — no changes to the subprocess
runtime, no breakage for existing plugins.

---

## 7. References

- `docs/PLUGIN_DESIGN.md` — original three-path comparison
  (subprocess vs WASM vs dlopen) from 2026-05-08.
- `docs/ROADMAP_v2.md` Theme G — v2 plugin runtime expansion
  context.
- wasmtime: https://wasmtime.dev/, Component Model docs at
  https://component-model.bytecodealliance.org/
- WASI 0.2 (current stable) preview-3 status:
  https://github.com/WebAssembly/WASI
- extism: https://extism.org/
- Wasmer: https://wasmer.io/

---

## Appendix A — Why not `dlopen`+`abi_stable`?

`PLUGIN_DESIGN.md` §3.1 rejected `dlopen` for v1; that
verdict carries forward unchanged. The added crash-isolation
and sandbox shortcomings versus WASM are even sharper now
that the subprocess runtime is mature. If a future workload
needs nanosecond plugin call overhead, the answer is a
built-in node, not a `dlopen` runtime.
