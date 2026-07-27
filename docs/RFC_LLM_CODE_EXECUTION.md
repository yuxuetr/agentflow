# RFC: LLM-Generated Code Execution (`code_exec`)

- Status: **Decided — adopted, 2026-07-27.** S4.2 (`ContainerBackend`) is
  unblocked; the five design constraints below are binding on that
  implementation, not just advisory.
- Parent: [`RFC_CODE_EXECUTION_TRUST.md`](RFC_CODE_EXECUTION_TRUST.md) — this
  RFC is the one place that trust model's default ("llm-generated content is
  never directly executable") is allowed to be lifted, and only under the
  conditions this document sets.
- Tracking: `TODOs.md` §S4. S4.2 (`ContainerBackend`) is gated on this RFC
  being adopted; if rejected, S4 closes as DEFERRED and S0.2's containment
  (no tool chains llm-generated content into an execution channel) remains
  the permanent final state, not an interim one.
- Scope: whether to build `code_exec` at all, and if so, the design
  constraints it must satisfy. Concrete implementation is S4.2, tracked
  separately once/if this is adopted.

## Problem

Every trust-level fix in S0–S3 (script integrity hashes, per-skill dependency
envs, OS sandbox defaults) makes the *existing* tools (`file`, `script`,
`shell`) safer for their intended job: running content a human vetted before
the run started. None of them let an agent run code **the model just wrote**
in response to the current conversation — that path is deliberately closed
(`RFC_CODE_EXECUTION_TRUST.md`'s trust-level corollary: llm-generated content
may be stored or passed as data, never promoted to an execution channel).

That's a real capability gap relative to what "code interpreter" / "code
execution" tools in comparable products (ChatGPT Code Interpreter, Claude's
own code execution tool, OpenAI's Assistants API) offer: a sandboxed scratch
environment where the model can write and immediately run a script to
compute something, transform a file, or validate its own output — without a
human pre-authoring that script as a skill asset.

This RFC asks two questions in order:

1. **Should AgentFlow support this at all**, given it's a deliberate,
   scoped exception to a trust boundary the rest of the S-track spent four
   items hardening?
2. **If yes, what does "safe enough to be worth it" require** — the four
   design points TODOs.md's S4.1 entry names in advance: temp working
   directory lifecycle, default-no-network, artifacts returned through an
   explicit result channel (not ambient filesystem access), and approval
   integration.

## Non-goals

- This RFC does not design the S4.2 `ContainerBackend` implementation
  (rootless Podman / gVisor / Firecracker, `SandboxBackend` trait impl,
  egress allowlist proxy) — that's separate, deliberately deferred work if
  and only if this RFC is adopted.
- This RFC does not revisit `ScriptTool`/`ShellTool`'s existing trust model
  (S0–S1). `code_exec` is additive and structurally separate — see
  "Relationship to `ScriptTool`" below — not a relaxation of the existing
  tools' guarantees.
- This RFC does not cover non-code LLM outputs (image generation, workflow
  plans) — those are already governed by their own tool-specific containment
  (e.g. S0.3's dynamic-workflow registry audit).

## Case for adopting

- Closes a real capability gap: today, if a skill wants "let the model
  compute something programmatically," the only options are (a) a
  human-authored `script` tool asset covering every case in advance, or (b)
  nothing. A general `code_exec` tool covers the long tail of one-off
  computations no author anticipated.
- The trust model in `RFC_CODE_EXECUTION_TRUST.md` was written to make this
  exact lift possible later, deliberately, rather than as an accident — S4
  is a planned pressure-release valve, not scope creep discovered mid-flight.
- `agentflow-harness`'s hook/approval pipeline (`wrap_registry` +
  `HookConfig`, P-H.2/H3) already exists and already escalates
  `NonIdempotent` tool calls to `RequireApproval` under the production
  profile — `code_exec` slots into infrastructure that's already built and
  tested, not a new approval subsystem.
- `SandboxBackend`'s trait shape (tri-state enforcement, `SandboxScope` with
  the S3.2 resource-limit fields) was designed generally enough that a
  container/microVM backend is an additive impl, not a redesign.

## Case for rejecting (staying DEFERRED)

- Every prior S-item's job was to make it *harder* for llm-generated content
  to reach an execution channel. Shipping `code_exec` reopens exactly that
  channel, deliberately — the residual risk is qualitatively different from
  "we sandboxed shell better," because the code being run is untrusted by
  construction on every single invocation, not just when something goes
  wrong.
- Strong isolation (S4.2) is real infrastructure investment: a
  container/microVM backend, an egress allowlist proxy, lifecycle management
  for ephemeral workdirs — meaningfully more than the RLIMIT/seccomp/Landlock
  work S3 already shipped. If actual skill-author or operator demand for
  this doesn't exist yet, that investment has no near-term payoff.
- `os_sandbox` (S3.4) + Landlock/cgroups (S3.1/S3.2) already cover the
  "author-signed script needs to be resource- and path-contained" case well.
  The marginal case `code_exec` adds — the model writing its *own* code — is
  a materially different threat model (adversarial-by-default input, not
  occasionally-buggy trusted input), and deserves to wait for a concrete
  use case to pressure-test the design against, rather than being built
  speculatively.

## If adopted: design constraints

These four are load-bearing — TODOs.md named them as the RFC's required
coverage, and each maps directly to a way "just add a tool that runs
whatever string the model gives it" goes wrong.

### 1. `code_exec` is a new, independent tool — not `ScriptTool` reused

`ScriptTool`'s entire safety argument (S1) is that it only ever executes
files whose name and content hash were fixed in the manifest at install
time — that's *why* it's eligible for the author-signed trust tier. Routing
model-generated code through `ScriptTool` (even behind a flag) would mean
either breaking that invariant for every skill using `ScriptTool`, or
threading a parallel "but this call is different" code path through a tool
whose whole design assumes it isn't. `code_exec` must be its own `Tool` impl
with its own registration, its own capability requirements, and its own
`ToolMetadata` — sharing only the underlying `SandboxBackend`/`SandboxScope`
machinery S3 already built, never the trust classification.

### 2. Ephemeral working directory, explicit lifecycle

Each `code_exec` invocation (or, if the design allows a multi-call "session,"
each session) gets a fresh temp directory:

- Created immediately before the call, torn down immediately after — no
  persistence across calls unless a future extension explicitly designs for
  it (out of scope here).
- Never overlaps with the skill's own `scripts_dir`, any `file`-tool
  `allowed_paths`, or any other tool's working directory — this is the same
  "one tool's defaults must not leak into another tool's policy" corollary
  S0.2 already established, applied to a new tool instead of an existing one.
- Sized and lifetime-bounded (reuse S3.2's `max_memory_bytes`/`max_cpu_secs`
  pattern for disk: a `max_workdir_bytes`-shaped limit, enforced the same
  way — cgroup-backed on Linux, best-effort quota/rlimit on macOS).

### 3. Default no network

`code_exec`'s effective capability set starts at `{}` (no `Net`, no `Env`
beyond an explicit allowlist) — a skill or operator must opt a specific
invocation into network access the same way `SandboxScope`/`Capability`
already gate it elsewhere, not get it by default. This is stricter than
`ScriptTool`'s current default (which inherits whatever the policy's
`allowed_domains` grants) precisely because the code being run here was
never vetted by anyone. S4.2's egress allowlist proxy is what makes an
*opt-in* network grant safe enough to offer at all; until that exists,
network access for `code_exec` should not ship.

### 4. Artifacts return through an explicit result channel

The model never gets ambient read/write access to the invoking skill's own
files, other tools' scopes, or the host filesystem beyond its own ephemeral
workdir. Whatever the code produces — computed values, generated files —
comes back through the tool's own structured `ToolOutput` (text +
`ToolOutputPart::{Text,Image,Resource}`, the same typed-output shape
`agentflow-tools` already has), not by the model separately reading files
`code_exec` happened to leave on disk somewhere reachable. This keeps the
trust boundary legible: everything that crosses back into the conversation
did so through one auditable channel, not through incidental filesystem
adjacency.

### 5. Approval integration

`code_exec` registers as `ToolIdempotency::NonIdempotent` (running arbitrary
code is never idempotent by definition), which means under
`agentflow-harness`'s production profile (`wrap_registry` + `HookConfig`,
P-H.2) it's automatically escalated to `RequireApproval` — no new approval
mechanism needed, just correct registration against infrastructure that
already exists. `Session`/`Run`-scoped approval caching (already implemented
for other `NonIdempotent` tools) applies unchanged. Non-harness callers (bare
`AgentRuntime` without a harness wrapping the registry) get no approval gate
by default — same as every other tool today — so `code_exec` should ship
with skill-authoring guidance that it's a harness-mode-only tool in practice,
even though nothing in the type system enforces that today.

## Relationship to `ScriptTool`

| | `ScriptTool` | `code_exec` (proposed) |
|---|---|---|
| Trust tier of executed content | Author-signed (S1 hash-pinned) | LLM-generated (never vetted) |
| Working directory | Skill's own `scripts/`, persists across the skill's lifetime | Fresh per invocation/session, torn down after |
| Network | Inherits policy `allowed_domains` | Default none; opt-in only once S4.2's egress proxy exists |
| Isolation backend | `SandboxBackend` (seccomp+Landlock / sandbox-exec) | S4.2 `ContainerBackend` (stronger: container/microVM) — `ScriptTool`'s OS-sandbox tier is not sufficient once the input is adversarial by construction |
| Approval | Opt-in via harness `HookConfig`, same as any tool | Same mechanism, but `NonIdempotent` registration makes it the *expected* path, not opt-in |

## Decision

**Adopted, 2026-07-27.** AgentFlow will support LLM-generated code
execution via a new `code_exec` tool, subject to the five design
constraints above. S4.2 (`ContainerBackend` + `code_exec` tool
implementation) is next, tracked in `TODOs.md` §S4.2 with its own design
pass before implementation (isolation-backend selection in particular
needs its own evaluation, not a default pick).
