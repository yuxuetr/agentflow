# RFC Addendum: `agentflow-tool` Contract Split (T3.3)

- Status: **Proposed** — migration plan only, no code changed yet. Written
  per `TODOs.md` T3.3's instruction to scope the work before touching code.
- Parent: `docs/RFC_CRATE_ARCHITECTURE.md` §4 (the `agentflow-tool`* row was
  planned there and never executed — this addendum is that execution plan).
- Source: `docs/archive/PROJECT_EVALUATION_2026-07-29.md` §1 finding 1 (the
  architecture dimension's largest single gap).
- Scope: internal crate split only. No change to the `Tool` trait shape, the
  `type:` node-factory dispatch, or any public API signature — every moved
  item keeps its name and is re-exported from its current path.

## Problem

`agentflow-tools` is listed in `xtask`'s `ARCH_KERNEL_CRATES` (the L0
contract-kernel set `check-arch` enforces `kernel-isolation` over) — but it
is not actually kernel-shaped. Alongside the `Tool` contract it also carries
five full builtin implementations (`ShellTool`, `FileTool`, `HttpTool`,
`ScriptTool`, `CodeExecTool`) and four concrete OS-sandbox backends
(`sandbox-exec` profile generation, seccomp+Landlock+cgroup v2, a real
container-engine driver, a no-op). That is exactly the kind of "impl" code
a kernel crate is supposed to never hold (RFC §7 Law 1).

`kernel-isolation` still passes today only because `agentflow-tools` itself
has zero internal workspace dependencies (nothing to point outward at) — the
law checks kernel→non-kernel edges, not what a kernel crate is made of. The
real problem this hides is RFC §7 Law 4 ("runtimes never depend on each
other; only on contracts"), which `check-arch` cannot yet enforce and instead
tracks as two **latent** (informational, non-failing) edges:

```
◦ latent: agentflow-agents  -> agentflow-tools will break law 4 runtime→impl
◦ latent: agentflow-harness -> agentflow-tools will break law 4 runtime→impl
```

Both `agentflow-agents` and `agentflow-harness` are runtimes (RFC §3). Their
`[dependencies]` on `agentflow-tools` are, today, dependencies on a crate that
bundles five OS-level tool implementations neither runtime should need to
compile against, let alone couple to.

## Empirical finding: the coupling is already contract-only in production code

Before proposing a shape, every `agentflow_tools::` import in `agentflow-agents`
and `agentflow-harness` was inventoried (`grep -rn "agentflow_tools::"`,
production `src/` only, examples and inline `#[cfg(test)]` modules excluded):

**`agentflow-agents/src/`** — exclusively: `Tool`, `ToolError`, `ToolOutput`,
`ToolOutputPart`, `ToolMetadata`, `ToolIdempotency`, `ToolRegistry`. Zero
references to any builtin tool or sandbox backend.

**`agentflow-harness/src/`** — exclusively: `Tool`, `ToolError`,
`ToolIdempotency`, `ToolMetadata`, `ToolOutput`, `ToolPermission`,
`ToolPermissionSet`, `ToolPolicy`, `ToolSource`. The one exception
(`agentflow_tools::builtin::CodeExecTool::new()`) is inside
`hooks_runtime.rs`'s own `#[cfg(test)] mod tests` — a unit test proving the
production approval-escalation path against the real shipped `code_exec`
tool, not production code itself.

**`agentflow-agent-spi`** (an L0 kernel crate — this is the one that matters
most, since a kernel crate depending on impl code would be an active Law-1
violation the moment it's classified correctly) uses exactly: `Tool`,
`ToolRegistry`, `Capability`, `CapabilityDecisionEntry`, `EffectiveCapabilities`,
`SandboxStatus`, `SandboxEnforcement`, `ToolOutputPart`. Also contract-only.

This means the split is **lower-risk than a typical "genuinely fused
runtime" migration**: no runtime logic in either crate actually needs a
concrete tool or sandbox backend. The only real work is moving the contract
surface into its own crate and repointing three `Cargo.toml` files — not
disentangling tangled logic.

## Measured partition (per-file, by what each item is consumed for)

| Item | File | Destination | Why |
|---|---|---|---|
| `Tool`, `ToolCall`, `ToolDefinition`, `ToolIdempotency`, `ToolMetadata`, `ToolOutput`, `ToolOutputPart`, `ToolPermission`, `ToolPermissionSet`, `ToolSource` | `tool.rs` | **contract** | the trait itself + its DTOs |
| `Capability`, `CapabilityDecisionEntry`, `EffectiveCapabilities`, `GrantSource` | `capability.rs` | **contract** | consumed by `agent-spi` directly |
| `ToolRegistry` | `registry.rs` | **contract** | holds `Arc<dyn Tool>`; no builtin references outside its own `#[cfg(test)]` module |
| `ToolError` | `error.rs` | **contract** | the trait's error type |
| `ToolPolicy`, `ToolPolicyDecision` | `policy.rs` | **contract** | pure decision logic over `ToolMetadata`/`Capability`, no backend deps |
| `PluginPolicy` + friends | `plugin_policy.rs` | **contract** | depends only on `SecurityProfile` |
| `SecurityProfile` + defaults structs | `security_profile.rs` | **contract** | config-shape used by callers deciding defaults, not backend-specific |
| `SandboxBackend` (trait), `SandboxScope`, `SandboxStatus`, `SandboxEnforcement`, `SandboxError` | `sandbox/backend.rs` (split) | **contract** | the trait + DTOs `Tool::sandbox_status()` and `agent-spi` return/consume; see note below |
| `default_backend()` | `sandbox/backend.rs` (split) | **builtin** | concretely dispatches to `macos`/`linux`/`noop` — impl, not contract |
| `SandboxPolicy`, `NetworkAddressClass` | `sandbox/policy.rs` | **builtin** | the in-process allow-list `ShellTool`/`FileTool`/`HttpTool` construct against; **no runtime or kernel crate references it in production code** (confirmed: only `agentflow-agents`' *examples* touch it) |
| `MacosSandboxExecBackend`, `LinuxSeccompBackend`, `ContainerBackend`, `NoopSandboxBackend` | `sandbox/{macos,linux,container,noop}.rs` | **builtin** | concrete backends |
| `ShellTool`, `FileTool`, `HttpTool`, `ScriptTool`, `CodeExecTool` | `builtin/*.rs` | **builtin** | concrete tools |

`sandbox/backend.rs` is the one file that splits down the middle: the trait
+ DTOs move, `default_backend()` (which `use`s the concrete backend modules)
stays. This mirrors the RFC's own precedent — `agentflow-graph`/`agentflow-core`
already split IR-type from executor-logic within what was one file's worth of
concerns.

Rough size: **~3,450 lines** move to the new contract crate (`tool.rs`,
`capability.rs`, `registry.rs`, `error.rs`, `policy.rs`, `plugin_policy.rs`,
`security_profile.rs`, the `sandbox/backend.rs` trait+DTO half,
`sandbox/mod.rs`'s re-exports); **~4,650 lines** (the five builtin tools + four
sandbox backends + `default_backend()`) stay in `agentflow-tools`.

## Decision

**Split into two crates**, following the exact shape already used for the
`agentflow-nodes` / `agentflow-nodes-ai` split (`docs/RFC_NODES_DECOMPOSITION.md`)
and the re-export-compatibility technique from R1.1 (`LlmTraceContext` sinking
into `agentflow-value`):

1. **`agentflow-tool`** (new, singular — matches the RFC §4 table's proposed
   name) — the contract crate: `Tool`, `ToolRegistry`, `ToolMetadata`,
   `ToolError`, `Capability`, `ToolPolicy`, `PluginPolicy`, `SecurityProfile`,
   `SandboxBackend` (trait) + `SandboxStatus`/`SandboxEnforcement`/
   `SandboxScope`/`SandboxError`. Depends on nothing but `serde`/`thiserror`/
   `async-trait`/`agentflow-value`-tier leaves. Genuinely belongs in
   `ARCH_KERNEL_CRATES`.
2. **`agentflow-tools`** (unchanged name) — reclassified as an L2 **Tool**
   tier crate (RFC §3), no longer in `ARCH_KERNEL_CRATES`. Depends on
   `agentflow-tool` and **re-exports everything from it** at the crate root
   (`pub use agentflow_tool::*;`), so every existing
   `use agentflow_tools::{Tool, ToolRegistry, ...}` in `agentflow-cli`,
   `agentflow-server`, `agentflow-worker`, `agentflow-skills`,
   `agentflow-config`, `agentflow-nodes`, `agentflow-rag` — the eight crates
   that depend on `agentflow-tools` today and genuinely need the concrete
   builtins — compiles completely unchanged. Zero edits needed in any of
   those eight crates.

### Why re-export rather than update every call site

`agentflow-agents` and `agentflow-harness` (and their examples) are the
*only* two crates that get a real edit — their `[dependencies]` entry moves
from `agentflow-tools` to `agentflow-tool`, and their contract-only imports
(`use agentflow_tools::{Tool, ...}`) become `use agentflow_tool::{Tool, ...}`.
Every other of the eight consumer crates keeps depending on `agentflow-tools`
(builtin) exactly as today, since they need the concrete tools; the
re-export means their source is untouched. This is the same trade this
codebase already made three times (`agentflow-tools` re-exporting
`agentflow_async_util::{retry, timeout}`; the P-A1.1/2.1/2.3/2.4 edges in
`ARCH_ALLOWLIST`'s own comments) — it is the established local convention,
not a new pattern.

### The one test-only back-edge

`registry.rs`'s own `#[cfg(test)]` module and `hooks_runtime.rs`'s
`production_profile_escalates_code_exec` test construct a real `ShellTool` /
`CodeExecTool` to exercise the registry/approval machinery against a real
tool, not a hand-rolled test double. After the split, `agentflow-tool`'s own
test module needs `agentflow-tools` (builtin) as a `[dev-dependencies]`
entry — a dev-only edge back from the new contract crate to the builtin
crate that depends on it. `[dev-dependencies]` are explicitly excluded from
`check-arch`'s graph (`xtask/src/main.rs`'s own comment: "test-only and do
not shape the shipped dependency graph") and this exact shape (a kernel/
contract crate taking its own downstream as a dev-dependency for a realistic
integration-style test) already exists for `agents→core` and `harness→agents`
per `ARCH_ALLOWLIST`'s comments — not a new risk class.

## Resulting edges (vs the latent map)

| Latent edge (today) | After split |
|---|---|
| `agentflow-agents  → agentflow-tools` (law 4 runtime→impl) | `agentflow-agents → agentflow-tool` (contract; edge removed from `ARCH_LATENT_EDGES`, not just re-classified) |
| `agentflow-harness → agentflow-tools` (law 4 runtime→impl) | `agentflow-harness → agentflow-tool` (contract; same) |
| `agentflow-agent-spi → agentflow-tools` (not currently tracked as latent, since agent-spi wasn't in the runtime set — but is a kernel→"kernel" edge today only because `agentflow-tools` is misclassified as kernel) | `agentflow-agent-spi → agentflow-tool` (kernel→kernel; correctly inert either way, but now honestly kernel→kernel rather than kernel→(kernel-that's-actually-impl)) |

`ARCH_KERNEL_CRATES` changes from:
```rust
const ARCH_KERNEL_CRATES: &[&str] = &[
  "agentflow-value", "agentflow-graph", "agentflow-store-spi",
  "agentflow-agent-spi", "agentflow-async-util", "agentflow-tools",
];
```
to:
```rust
const ARCH_KERNEL_CRATES: &[&str] = &[
  "agentflow-value", "agentflow-graph", "agentflow-store-spi",
  "agentflow-agent-spi", "agentflow-async-util", "agentflow-tool",
];
```
`CLAUDE.md`'s "L0 Contract Kernel" list gets the same rename.

## Sequencing (mechanical steps, each keeping `cargo test` + `check-arch` green)

1. `cargo new --lib agentflow-tool`, add to workspace `members`, wire
   `[workspace.package]` inheritance.
2. `git mv` the nine contract-tier files (`tool.rs`, `capability.rs`,
   `registry.rs`, `error.rs`, `policy.rs`, `plugin_policy.rs`,
   `security_profile.rs`) into `agentflow-tool/src/`; split
   `sandbox/backend.rs` into the trait+DTO half (moves) and
   `default_backend()` (stays, temporarily referencing the not-yet-moved
   concrete backends via a `pub(crate)` shim if needed for the interim commit).
3. `agentflow-tools/Cargo.toml`: add `agentflow-tool = { path = "../agentflow-tool" }`
   as a real dependency; `agentflow-tools/src/lib.rs` becomes
   `pub use agentflow_tool::*;` plus its own `builtin`/`sandbox` (backend
   impls only) modules.
4. Repoint `agentflow-agents/Cargo.toml` and `agentflow-harness/Cargo.toml`
   (`[dependencies]`) from `agentflow-tools` to `agentflow-tool`; fix their
   `use agentflow_tools::...` imports (contract-only, per the inventory
   above) to `use agentflow_tool::...`; move their example-only /
   `#[cfg(test)]`-only builtin usages to `[dev-dependencies]` on
   `agentflow-tools`.
5. Add `agentflow-tools = { path = "../agentflow-tools" }` to
   `agentflow-tool/Cargo.toml`'s `[dev-dependencies]` for the
   registry-with-a-real-`ShellTool` tests.
6. Update `xtask`'s `ARCH_KERNEL_CRATES` and prune the two now-resolved
   `ARCH_LATENT_EDGES` rows (the gate's own staleness check will fail loudly
   if this step is missed, per its documented self-maintenance contract).
7. Update `CLAUDE.md`'s L0 Contract Kernel crate list and the
   `agentflow-agents`/`agentflow-harness`/`agentflow-tools` crate-responsibility
   paragraphs.
8. Full-workspace `cargo fmt` / `cargo clippy --workspace --all-features
   --all-targets` / `cargo test --workspace --all-features` / `cargo run -p
   xtask -- check-arch` after each of steps 3–6, not just at the end — matches
   how every other T-item this session verified incrementally rather than in
   one big-bang commit at the end.

## Effort estimate and risk

- **Mechanical, not exploratory**: the empirical inventory above already
  answers "what does each runtime actually need" — there is no design
  ambiguity left to resolve during implementation, only file moves and
  `Cargo.toml`/`use` edits.
- **Estimated size**: 2 new/moved crate directories, ~4 `Cargo.toml` edits,
  ~10-15 `use` statement edits across `agentflow-agents`/`agentflow-harness`
  (contract-only imports, mechanical rename), 1 `xtask` edit, 2 doc edits.
  No new tests are strictly required (the moved files bring their existing
  unit tests with them unchanged) — though a small `check-arch` regression
  test asserting the two latent edges are gone would be worth adding.
- **Main risk**: incomplete `use agentflow_tools::` → `use agentflow_tool::`
  rewrites in `agentflow-agents`/`agentflow-harness` failing to compile
  loudly (safe — caught immediately by `cargo check`, not a silent runtime
  bug) versus subtly changing behavior (not expected — no logic moves, only
  declarations and paths).
- **Nothing here requires design judgment calls** the way T3.1 did (retry
  safety semantics) — the boundary is already determined by what's
  contract-shaped versus impl-shaped, confirmed against actual usage rather
  than assumed.
