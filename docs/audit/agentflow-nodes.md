# Audit: agentflow-nodes

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-nodes/
**Crate version**: 0.2.0
**Layer**: L2 (Capability Adapter)
**Stability tier**: not declared in `docs/STABILITY.md` (treated as Beta/L2 capability adapter)

## Scope summary

`agentflow-nodes` ships the built-in `AsyncNode` library that workflows
compose. The crate has 15 node modules under `src/nodes/` (LLM, HTTP,
File, Template, Batch, Conditional, While [empty], MCP, RAG, ARXiv,
MarkMap, ASR, TTS, TextToImage, ImageToImage, ImageEdit,
ImageUnderstand), plus `common/` helpers, an `error` module, and a
declared-but-unused `NodeFactory` registry. Real construction of nodes
from config happens in `agentflow-cli/src/executor/factory.rs` — the
in-crate `factory_traits.rs` is never implemented anywhere in the
workspace and is effectively dead code. No `tests/` or `examples/`
directories exist; coverage lives entirely in `#[cfg(test)] mod
tests` inside each source file and `benches/node_latency.rs`.

## Findings

### CRITICAL

- [C1] TextToImage silently downgrades to MOCK on real-API failure
  — `agentflow-nodes/src/nodes/text_to_image.rs:397-441`
  **What**: When `execute_real_image_generation` returns `Err(_)` (auth
  failure, network error, quota exhausted, etc.), control flows into
  `execute_mock_image_generation`, which returns a hardcoded 1x1 PNG
  data URI (`...iVBORw0KGgo...`) or a fake URL
  (`https://example.com/generated-image-mock.png`). The caller sees a
  successful `Ok(outputs)` and proceeds. The mock branch is reachable
  both in the timeout path (line 408-414) and the no-timeout path (line
  434-438). The original error is swallowed entirely (`Err(_)`).
  **Why it matters**: A production workflow that asks for an image and
  receives a 1x1 placeholder will write that placeholder downstream
  (file save, MarkMap embed, vision-understand input, end-user reply),
  with no signal in logs beyond a `(MOCK - API key not available)`
  println. There is also no way to opt out — the fallback is
  unconditional. This violates the "fail fast" principle from
  `CLAUDE.md` and breaks the upstream contract for every consumer.
  **Fix**: Remove the mock fallback. Make `execute_mock_image_generation`
  test-only (`#[cfg(test)]` or a separate `MockTextToImageNode` type).
  If a fallback is genuinely desired, gate it on an explicit
  `fallback_to_mock: bool` config field that defaults to `false`, and
  surface the original provider error in the output (or as a separate
  `error` key) so downstream nodes can branch on it.

- [C2] HTTP node has no timeout, URL allowlist, or SSRF guard
  — `agentflow-nodes/src/nodes/http.rs:14-66`
  **What**: `HttpNode::execute` constructs `reqwest::Client::new()`
  (line 21) with the library default (no timeout, no proxy override,
  no redirect cap, no URL validation). The `url` input is passed
  through to `client.get/post/...` verbatim. No allowlist, no scheme
  check, no rejection of private/loopback/link-local ranges.
  **Why it matters**: An LLM-driven workflow that builds the `url`
  input from a prompt (the documented pattern) can be coerced into
  hitting `http://169.254.169.254/latest/meta-data/` (AWS IMDS),
  `http://localhost:8500/v1/agent/services` (Consul), or any internal
  service the worker can reach. With no timeout, a slow target can
  pin the executor; with no redirect cap, an attacker-controlled URL
  can chain redirects to bypass any external WAF.
  **Fix**: Build the client with `reqwest::Client::builder().timeout(...)
  .redirect(redirect::Policy::limited(10)).build()` (cache it as a
  `OnceLock` since the same client is reused per call). Add a
  pluggable `UrlValidator` trait or a feature-gated default policy
  that rejects RFC1918 / loopback / link-local / IPv6 ULA unless an
  explicit `allow_private: true` opt-in is set in the node config.
  Document the SSRF posture in `README.md` under "Security".

- [C3] File node has no path-traversal or sandbox enforcement
  — `agentflow-nodes/src/nodes/file.rs:14-54`
  **What**: `FileNode::execute` takes a `path` input string and passes
  it directly to `tokio::fs::read_to_string` / `tokio::fs::write` /
  `tokio::fs::create_dir_all` (write also auto-creates any missing
  parent). No workspace sandboxing, no symlink resolution check, no
  rejection of `..` traversal, no rejection of absolute paths,
  no rejection of `/etc/passwd` / `/proc/self/environ` reads, no
  rejection of `~/.ssh/authorized_keys` writes.
  **Why it matters**: Same threat model as C2 — when an LLM is in the
  loop, the `path` value is attacker-influenced. The `agentflow-tools`
  crate already ships `SandboxPolicy` and `FileTool` with these
  protections; the built-in `FileNode` bypasses all of it. Workflows
  that mix LLM output with `file` nodes (e.g., the documented
  "save generated report to {{output_path}}" pattern) are vulnerable
  to arbitrary file read/write.
  **Fix**: Either (a) deprecate `FileNode` in favor of routing through
  `agentflow-tools::FileTool` with its `SandboxPolicy`, or (b) add an
  optional `root: PathBuf` field to `FileNode` and reject any resolved
  path (after `canonicalize`) that escapes `root`. Symlinks should be
  resolved before the prefix check.

### MAJOR

- [M1] Stale regex compilation in hot path with `.unwrap()`
  — `agentflow-nodes/src/nodes/arxiv.rs:135, 175, 269-272`
  **What**: `fetch_arxiv_paper`, `search_arxiv`, and
  `simplify_latex_content` all call `Regex::new(...).unwrap()` on
  every invocation. The patterns are constant strings, so the
  `unwrap()` is defensible, but compiling on the hot path is wasteful
  and violates the "no unwrap in prod" rule from `CLAUDE.md`.
  **Fix**: Move each regex into a `static` with `once_cell::sync::Lazy`
  or `std::sync::LazyLock` (Rust 1.80+). This both removes the
  `.unwrap()` per call and amortises the compile cost.

- [M2] `text_to_image.rs` has multiple production `.unwrap()` on
  serialization and config access
  — `agentflow-nodes/src/nodes/text_to_image.rs:189, 211, 338-339, 401, 410, 431, 437`
  **What**: Line 189: `serde_json::to_value(&self.response_format).unwrap()`
  (production path in `create_image_config`). Line 211: same for
  `style_ref`. Lines 401/410/431/437: `config.as_object().unwrap()`
  (the local just constructed `Value::Object(...)`, so it's safe in
  practice, but the type system doesn't capture that — return
  `serde_json::Map` directly from `create_image_config` instead of
  `Value`). Lines 338-339 in the mock path also unwrap — once C1 is
  fixed those become test-only.
  **Fix**: Return `serde_json::Map<String, Value>` from
  `create_image_config`; replace `to_value(...).unwrap()` with
  `serde_json::to_value(...).map_err(|e| AgentFlowError::AsyncExecutionError{...})?`.

- [M3] `template.rs` global Tera instance with `Mutex::lock().unwrap()`
  — `agentflow-nodes/src/nodes/template.rs:14-25, 88`
  **What**: A single global `OnceLock<Mutex<Tera>>` serializes every
  template render across the entire process, and `tera.lock().unwrap()`
  will panic on poisoning. Two issues stack: (1) all template renders
  in a workflow contend on one mutex, which contradicts the concurrent
  scheduler's design; (2) a panic in one render poisons the mutex and
  bricks every subsequent template render in the process.
  **Fix**: Use `Tera::default()` per-node-construction (Tera is `Clone`
  and `Send`), or wrap in `tokio::sync::Mutex`, or use `parking_lot::Mutex`
  which doesn't poison. The custom filters/functions can be registered
  once into a template that is then cloned per node — this is the
  intended idiomatic pattern. The current global is a perf and
  resilience footgun.

- [M4] `NodeFactory` / `NodeRegistry` declared but never implemented
  — `agentflow-nodes/src/factory_traits.rs` (entire file)
  **What**: The `NodeFactory` trait, `NodeRegistry`, `NodeConfig`, and
  `ResolvedNodeConfig` types are public-exported from `lib.rs:21` but
  zero implementations exist in the workspace. The CLI's actual node
  construction lives in `agentflow-cli/src/executor/factory.rs`,
  which uses a parallel, incompatible API (`NodeDefinition` →
  `Arc<dyn AsyncNode>`). The doc comment claims the trait is "for
  agentflow-config" but that crate was removed (see Cargo.toml line 37).
  **Fix**: Either delete `factory_traits.rs` and the matching exports,
  or rewrite it so the CLI's factory actually uses it (the trait is
  the right shape; the CLI just doesn't depend on it). Leaving dead
  public API in a 0.2 crate locks in a maintenance cost and confuses
  external integrators reading docs.rs.

- [M5] Markmap node calls a hardcoded third-party Cloudflare Worker by default
  — `agentflow-nodes/src/nodes/markmap.rs:27`
  **What**: `MarkMapConfig::default()` sets `api_url:
  Some("https://markmap-api.jinpeng-ti.workers.dev".to_string())`.
  This is a personal Cloudflare Worker subdomain. Every `markmap` node
  in any AgentFlow deployment that doesn't override the URL sends its
  full markdown payload (which may contain LLM outputs, internal
  document content, etc.) to a single non-Anthropic, non-AgentFlow
  endpoint with no SLA, no privacy contract, and no auth.
  **Why it matters**: (1) Data exfiltration vector (markdown can
  include confidential workflow state); (2) availability dependency
  on a personal account; (3) reputational risk if `jinpeng-ti.workers.dev`
  serves anything malicious in the future.
  **Fix**: Either ship the markmap rendering in-process (the markmap.js
  toolchain runs in Node, so this is non-trivial; document the security
  trade-off and make the URL `None` by default forcing explicit
  opt-in), or stand up an AgentFlow-controlled `markmap-api.agentflow.dev`
  endpoint with documented data policy, or strip this node entirely
  pending a real solution.

- [M6] `while.rs` is an empty file
  — `agentflow-nodes/src/nodes/while.rs` (0 lines, not included in `mod.rs`)
  **What**: `src/nodes/while.rs` exists with zero bytes, is not
  registered in `src/nodes/mod.rs`, and is not documented. The
  `WhileNode` control-flow is handled by `agentflow-core` (per
  `README.md` "Control Flow Nodes" section), so the file is dead.
  **Fix**: Delete the empty file. If a stub doc-only re-export is
  desired, add a one-line `//! See [`agentflow_core::node_type::NodeType::While`].`
  with the `#[cfg]` gate.

- [M7] Feature-gate matrix doesn't match documented capability list
  — `agentflow-nodes/Cargo.toml:13-22`, `src/nodes/mod.rs:6-41`
  **What**: `CLAUDE.md` says "Feature-gated; factory pattern in
  `factory_traits.rs`" and lists asr/tts/text_to_image/image_*/markmap/arxiv
  as separate node types that should be gated. In reality, all of
  these are compiled unconditionally:
  - `image_edit`, `image_to_image`, `image_understand`, `text_to_image`,
    `asr`, `tts`, `arxiv`, `markmap` are declared without any
    `#[cfg(feature = ...)]` in `mod.rs:6-14, 33-34`.
  - The `llm = []` feature in Cargo.toml is an empty placeholder
    (doesn't actually gate anything in `mod.rs`; only gates the
    `pub use` in `lib.rs:17`).
  Downstream consumers can't pick a minimal build — pulling in the
  crate at all transitively pulls reqwest (for arxiv/markmap),
  flate2, tar, regex, base64, and the full `agentflow-llm` provider
  stack regardless of features.
  **Fix**: Add `#[cfg(feature = "image")]`, `#[cfg(feature = "audio")]`,
  `#[cfg(feature = "arxiv")]`, `#[cfg(feature = "markmap")]` gates and
  matching Cargo features; gate the corresponding `agentflow_llm`
  modality imports behind them too. Then update `default = [...]`
  to keep the today-default behaviour and document the trim path.

- [M8] No `tests/` directory; integration coverage of the multi-node
  surface is absent
  — workspace layout (no `agentflow-nodes/tests/`)
  **What**: Every test in this crate is a same-file unit test. There
  are zero integration tests exercising (a) the factory_traits dynamic
  dispatch, (b) the combination of multiple nodes via a `Flow`,
  (c) failure modes that cross node boundaries (e.g., what happens
  when an HTTP node times out feeding into a Template node). The
  scheduler tests live in `agentflow-core`; consumer tests live in
  `agentflow-cli`; the node-level integration gap is uncovered.
  **Fix**: Add `agentflow-nodes/tests/` with at least:
  `template_into_llm.rs`, `http_failure_propagation.rs`,
  `batch_with_failing_child.rs`, `conditional_branch_routing.rs`.
  Existing integration patterns in `agentflow-core/tests/` are good
  references.

- [M9] `arxiv.rs` and `common/utils.rs` use bare `reqwest::get` with
  no timeout and no proxy control
  — `agentflow-nodes/src/nodes/arxiv.rs:160, 202`, `src/common/utils.rs:96`
  **What**: `reqwest::get(...)` uses the global default client with no
  timeout, no redirect cap, and (per the user's documented test
  guideline in `~/.claude/CLAUDE.md`) no `.no_proxy()` — though for
  production code that's fine, the lack of timeout means a slow
  arxiv mirror or a stuck data-URI fetch in `load_bytes_from_source`
  hangs the calling node indefinitely.
  **Fix**: Build a module-level `OnceLock<reqwest::Client>` with
  `.timeout(Duration::from_secs(30))` and route all internal HTTP
  calls through it. The MarkMap node already does this pattern at
  `markmap.rs:147-152`; lift it to a shared helper in `common/`.

### MINOR

- [m1] `LlmNode::execute` runs `AgentFlow::init().await` on every
  call (`llm.rs:21`). The init is idempotent (`OnceCell` internally),
  but the unnecessary `.await` adds an allocation per request. Move
  init to `AsyncNode::prepare` or assume the runtime has initialised
  it once at startup.

- [m2] `println!` is used throughout for progress logging (LLM, MCP,
  RAG, Template, Image*, ASR, TTS, Batch, Conditional, Arxiv —
  dozens of sites). Per the project's observability story, this
  should be `tracing::info!` / `tracing::debug!` so it integrates
  with the OTel span pipeline, can be filtered, and doesn't pollute
  stdout in `--output json` CLI mode.

- [m3] `mcp.rs:176` warns on disconnect failure via `eprintln!`
  inside an `.ok()` chain. The error is dropped after printing; use
  `tracing::warn!` and consider whether a disconnect failure should
  bubble up.

- [m4] `batch.rs:116` — `h.get("output").unwrap().clone()` on the
  child node's result assumes every child writes an `output` key.
  If a child writes `"results"` or anything else, this panics.
  Replace with `.ok_or_else(|| AgentFlowError::AsyncExecutionError {
  message: "batch child did not produce 'output' key" })?`.

- [m5] `text_to_image.rs:140-151` and `image_understand.rs:55-61` and
  `image_edit.rs:53-59` and `image_to_image.rs:59-65` and `markmap.rs:73-89`
  and `arxiv.rs:116-130` all reimplement an ad-hoc `{{key}}` template
  substitution instead of using the shared Tera path that `TemplateNode`
  already wires up. Five copies of the same logic with three different
  placeholder formats (`{{x}}`, `{{ x }}`, both).
  Fix: factor into a `common::placeholder::resolve(template, inputs)`
  helper used by every node that does string templating, or push
  consumers toward composing with `TemplateNode` upstream.

- [m6] `common/tera_helpers.rs:26` — `serde_json::Number::from_f64(f).unwrap()`.
  `f64::NAN` and `f64::INFINITY` make `from_f64` return `None`. Replace
  with `.unwrap_or_else(|| TeraValue::Null)` and return early at the
  call site.

- [m7] `arxiv.rs:252` — `if content.contains(r"\\begin{document}")` is
  the source-code form of `\\begin{document}` which matches the
  literal six characters `\\begin{document}`, not `\begin{document}`.
  This means the "main content" detection never triggers and the code
  always falls into the "Fallback to all content" branch on line 259.
  This is a silent bug.

- [m8] `arxiv.rs:155` — uses `http://export.arxiv.org/api/query` (no
  TLS) when arXiv supports HTTPS. Switch to `https://`.

- [m9] `image_understand.rs:65-68`, `image_to_image.rs`, `image_edit.rs`
  use `MultimodalMessage` / modality dispatch but don't propagate
  timeout. Only `text_to_image.rs` has a `timeout_ms` field. Add the
  same to the other long-running providers for cancellation safety.

- [m10] `rag.rs:412` — when calling `delete_collection`, the code
  still creates an `OpenAIEmbedding` "just for connection". Qdrant
  delete-collection doesn't need an embedder; this requires an
  OpenAI API key for a destructive op that doesn't use one. Refactor
  `QdrantStore` (in `agentflow-rag`) to allow embedder-free
  construction for control-plane operations.

- [m11] Public configs (`MarkMapConfig`, `StyleReference`,
  `AudioResponseFormat`, `ASRResponseFormat`, `ImageResponseFormat`,
  `TextToImageNode`, `ImageEditNode`, `ImageToImageNode`,
  `ImageUnderstandNode`, `ASRNode`, `TTSNode`, `RAGNode`) have very
  thin `///` doc coverage — mostly one-line type-level comments,
  rarely per-field. `cargo doc -p agentflow-nodes -- -D missing-docs`
  would fail loudly. CLAUDE.md commits to "All public APIs documented
  with `///` comments".

- [m12] `text_to_image.rs:139-151` resolves placeholders only for
  `FlowValue::Json(Value::String)` — `File` and `Url` variants in
  `inputs` are silently ignored. The other image nodes use
  `flow_value_to_string` which handles all three; align the behaviour.

- [m13] `Cargo.toml:43` pins `handlebars = "4.0"` as an optional dep,
  but `template.rs` uses Tera, not Handlebars. The dep is unused
  (the `template` feature flag activates it but no code imports it).
  Remove `handlebars` and the now-orphaned alias.

- [m14] `Cargo.toml:50` pulls `tokio = { version = "1.0", features =
  ["full"] }` — the "full" feature includes signal handling, process
  management, fs, net, etc. A library crate should declare only what
  it uses (`rt`, `sync`, `macros`, `time`, `fs`, `net`); "full"
  inflates dependent binary sizes unnecessarily.

### POSITIVE OBSERVATIONS

- The `common/utils.rs` `load_data_uri_from_source` / `load_bytes_from_source`
  helpers cleanly unify the three `FlowValue` variants
  (`Json(String)`, `File`, `Url`) into one source-resolution path
  used by every image / audio node. This is exactly the right
  abstraction for multimodal nodes.

- `template.rs` has a thorough Tera test suite (15 tests covering
  conditionals, loops, filters, length, object/array access, default
  filter, math, complex multi-feature template) — best test coverage
  in the crate.

- `factory_traits.rs` (despite being unused) has the right shape for a
  dynamic-dispatch registry; consolidating the CLI's parallel factory
  onto this trait would be a clean refactor rather than a rewrite.

- The `benches/node_latency.rs` covers the three pure-compute nodes
  (template, conditional, file) with three sizes each, hooks into
  bench-gate via the documented `<group>/<variant>/<param>` schema,
  and explicitly notes why LLM/HTTP/MCP nodes aren't in the gate.

- `text_to_image.rs:457-481` ships well-named preset constructors
  (`artistic_generator`, `quick_generator`, `batch_generator`) — a
  good ergonomic pattern other nodes should adopt.

- `rag.rs:109-114` correctly compiles a feature-disabled stub that
  returns a clear `ConfigurationError` rather than failing to compile
  or panicking — the right default for an optional feature.

## Metrics

- Source files: 18 `.rs` files (15 node modules + lib/error/factory + 2 common)
- Lines of code: 4,707 total (largest: rag.rs 623, text_to_image.rs 504, template.rs 382, mcp.rs 331, arxiv.rs 280)
- Node types implemented: 15 active (LlmNode, HttpNode, FileNode, TemplateNode, BatchNode, ConditionalNode, MCPNode, RAGNode, ArxivNode, MarkMapNode, ASRNode, TTSNode, TextToImageNode, ImageToImageNode, ImageEditNode, ImageUnderstandNode) + 1 empty file (`while.rs`)
- Test files: 13 unit (in-file `#[cfg(test)]`) + 0 integration (no `tests/` dir)
- Examples: 0 (no `examples/` dir)
- Benchmarks: 1 (`benches/node_latency.rs`, covers template/conditional/file)
- `unwrap()/expect()` in non-test code: ~16, headline offenders:
  1. `arxiv.rs:135, 175, 269, 270, 271, 272` — six `Regex::new(...).unwrap()` hot-path compiles
  2. `text_to_image.rs:401, 410, 431, 437` — four `config.as_object().unwrap()`
  3. `template.rs:88` — `tera.lock().unwrap()` (poisoned mutex panic)
  4. `text_to_image.rs:189, 211` — `serde_json::to_value(...).unwrap()`
  5. `common/tera_helpers.rs:26` — `Number::from_f64(f).unwrap()` (NaN/Inf panic)
- TODO/FIXME in code: 1 (only a doc-comment reference in `benches/node_latency.rs:13`)
- Public items missing rustdoc: estimated ~40+ public fields across `MarkMapConfig`, `StyleReference`, the image / audio / RAG / MCP node structs

## Recommendations (prioritized)

1. **Fix C1** (remove silent mock fallback in `TextToImageNode`) — this
   is a correctness bug shipping wrong data downstream. One-day fix.
2. **Fix C2 + C3 + M5** as a security pass: add timeout + URL allowlist
   to `HttpNode`, route `FileNode` through `agentflow-tools`'s sandbox
   policy, and replace the personal Cloudflare Worker default in
   `MarkMapNode` with either an in-process renderer or a required
   explicit URL. These three together close the LLM-in-the-loop
   attack surface.
3. **Fix M3** (`Tera` global mutex) — pick `parking_lot::Mutex` for
   the quick win or refactor to per-node Tera instances. Either
   eliminates the poisoning panic and the contention.
4. **Fix M1 + M2** (lazy-static the regexes, remove the
   `text_to_image.rs` unwraps) — small, mechanical, clears a
   significant chunk of the unwrap count.
5. **Decide M4 + M6**: either implement `NodeFactory` for every node
   and wire the CLI to use it, or delete the trait. Either way,
   delete the empty `while.rs`.
6. **Fix M7** (feature-gate the heavy nodes properly) before any 1.0
   tag — adding feature flags after stabilisation is a breaking change.
7. **Backfill M8** (integration tests under `agentflow-nodes/tests/`)
   targeting the cross-node failure modes.
8. **m7 silent bug** (`arxiv.rs:252` `\\begin{document}` doesn't
   match) — diagnose and fix; today the "main content" branch is
   unreachable.
9. **m13 + m14**: drop unused `handlebars` dep; trim `tokio` to the
   actually-used features. Small wins for downstream build times.

End of report.
