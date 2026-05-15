# Skill Validator Protocol

Status: design as of `P4.4 follow-up step 3`.
Crate (forthcoming impl): `agentflow-skills::validator` + wiring in
`agentflow-agents::eval::assertion::final_answer_matches_skill`.
Implements: the `final_answer_matches_skill` assertion variant from
`docs/AGENT_EVAL_FORMAT.md`, P4.4 follow-up step 1's known gap.

The eval harness's six-variant assertion DSL includes
`final_answer_matches_skill`, which defers to "the skill's bundled
validator". Today that validator hook is wired through
`AssertionContext::skill_validator: Option<&SkillValidator>` but no
skill manifest ever populates it — every use of the assertion fails
with `skill declares no validator`. This document is the v1 contract
that fills the gap.

## Goals

1. A skill can declare a validator **once**, alongside its persona /
   tools / knowledge, not per-eval-case.
2. The contract is **closed**: a fixed set of validator kinds keeps
   the trust surface auditable and the implementation footprint small.
3. Validators are **hermetic enough for CI** — they must not require
   a live LLM call or network access.
4. The protocol composes with the existing skill security profile
   (sandbox-exec / seccomp) when shell-style validators are used.
5. The serialisation is **stable**: future kinds bump
   `[validation.schema_version]` rather than breaking existing skills.

## Non-goals

- Multi-validator pipelines (run validator A *and* B). Operators who
  need that can write one wrapper script.
- Probabilistic / LLM-graded validators. Those live in the agent eval
  harness as additional `Assertion` variants (`regex` exists today;
  future `tool_called_with_value`, etc.), not in the skill manifest.
- Cross-skill shared validators. A validator is a property of one
  skill.

## v1 protocol

The manifest gains an optional top-level `[validation]` table. Every
validator declares a `kind` discriminator and a small set of
kind-specific fields:

```toml
# skill.toml — option 1: regex over final answer
[validation]
kind = "regex"
pattern = "(?i)^OK\\b"
```

```toml
# skill.toml — option 2: shell command, exit code = verdict
[validation]
kind = "command"
command = ["bash", "tests/validator.sh"]
timeout_secs = 5
working_dir = "."  # optional, relative to skill_dir; defaults to skill_dir
```

```toml
# skill.toml — option 3: explicitly declare "no validator"
[validation]
kind = "none"
```

Missing `[validation]` is treated as `kind = "none"` (today's
behaviour). The skill author opts in by adding the section.

The closed `kind` enum at v1: `none` | `regex` | `command`. Future
additions (e.g. `json_shape`, `contains_all`) bump
`[validation.schema_version]` (default `1`).

### Per-kind contracts

#### `none`

No validator. `final_answer_matches_skill` reports
`AssertionOutcome { passed: false, reason: "skill declares no
validator" }` — same as today.

#### `regex`

```toml
[validation]
kind = "regex"
pattern = "(?i)\\b(ok|done|success)\\b"
# optional knobs:
multiline = false   # default false; sets the regex `m` flag
dotall    = false   # default false; sets the regex `s` flag
```

Evaluation: `regex::Regex::is_match(final_answer)`. Compile errors
in the manifest are surfaced at `SkillLoader::validate` time, not at
eval time — bad regexes never reach the runner. Backed by the same
`regex` crate the rest of the workspace uses.

#### `command`

```toml
[validation]
kind = "command"
command = ["bash", "tests/validator.sh"]
timeout_secs = 5      # default 30; clamped to [1, 120]
working_dir = "."     # default skill_dir
env_allowlist = ["PATH", "LANG"]  # default
```

Wire protocol:

1. Harness writes the final answer to the child's stdin and closes
   it.
2. Child process exits within `timeout_secs`:
   - **Exit code 0** → pass.
   - **Exit code 1–124, 126, 127** → fail; stderr is captured into
     the case report's `runtime_error` field (truncated to 2 KiB).
   - **Exit code 125** is reserved for "validator could not run"
     (mirrors `git bisect run` convention) and reports as
     `AssertionOutcome { passed: false, reason: "validator
     unrunnable: <stderr>" }`.
   - **Timeout** → fail with `reason: "validator timed out after Ns"`.
3. Environment: stripped to `env_allowlist`. The default keeps `PATH`
   and `LANG` only — no `AGENTFLOW_*`, no API keys, no `HOME`
   override. Skills that need more must opt in explicitly.

Security:

- The validator command is subject to the same `security.os_sandbox`
  flag the skill's `shell` / `script` tools honor. Under
  `production` profile + `os_sandbox = true`, the command runs inside
  sandbox-exec (macOS) / seccomp (Linux), with read access to
  `working_dir` only.
- `agentflow doctor --profile production --backup-check` already
  fails the host when sandbox isn't enforcing, so production
  deployments that declare a `command` validator inherit the same
  fail-closed posture.
- The validator's command path goes through
  `security.mcp_command_allowlist` (today scoped to MCP commands —
  this proposal extends it to validator commands too, with the same
  allowlist semantics: a relative path resolves against PATH, but the
  resolved executable's basename must be in the allowlist or
  validation fails at `SkillLoader::validate` time).

### Validator surface in code

A new trait in `agentflow-skills::validator`:

```rust
pub trait SkillValidator: Send + Sync {
  /// `Some(true)` = answer accepted; `Some(false)` = rejected;
  /// `None` = could not run validator (treat as "skill declares no
  /// validator" by the assertion layer).
  fn validate(&self, final_answer: &str) -> Option<bool>;
}
```

Two built-in implementations:

```rust
pub struct RegexValidator { regex: regex::Regex }
pub struct CommandValidator {
  command: Vec<String>,
  timeout: Duration,
  working_dir: PathBuf,
  env_allowlist: Vec<String>,
  sandbox: SandboxStatus, // resolved from skill security profile
}
```

Plus a `NoValidator` zero-sized type that always returns `Some(false)`
with a "no validator declared" reason hook (kept separate from the
`Option<&SkillValidator>` `None` case so the assertion layer can
distinguish "skill explicitly has no validator" from "harness was
never given one").

A factory:

```rust
pub fn build_validator(
  manifest: &SkillManifest,
  skill_dir: &Path,
) -> Result<Option<Arc<dyn SkillValidator>>, SkillError>
```

`None` is returned for `kind = "none"` or absent `[validation]`. The
eval factory wires this into `AssertionContext::skill_validator`:

```rust
fn skill_validator<'a>(&'a self, case: &'a EvalCase) -> Option<BoxedSkillValidator<'a>> {
  let validator = self.skill_validators.get(&case.skill?)?;
  Some(Box::new(move |answer| Some(validator.validate(answer)?)))
}
```

## Synchronous vs async

The trait is intentionally **synchronous**: validators are short
(<10s by contract), often pure CPU (regex), and the assertion layer
runs them outside the agent loop. A synchronous trait avoids
forcing the assertion DSL to be async too, keeps backtraces shallow,
and lets the regex validator avoid an executor entirely.

The `command` validator's child process is awaited via
`tokio::process::Command` internally and the trait's `validate` is
called from a `tokio::task::block_in_place` block when the harness
already has a runtime. Detail of the implementation; not part of
the contract.

## CLI surface

`agentflow skill validate <skill-dir>` already exists. This proposal
extends it to:

1. Verify the `[validation]` section parses (and compiles, for `regex`).
2. For `kind = "command"`, verify the command's executable is in
   `mcp_command_allowlist` (which `skill validate` already enforces
   for MCP commands).
3. With `--check-validator <answer>`, actually run the validator
   against a sample answer and print the verdict. Useful for skill
   authors before publishing.

`agentflow skill inspect --explain-permissions` (P3.5) gains a new
section:

```
Validator:
  kind:        command
  command:     bash tests/validator.sh
  timeout:     5s
  working_dir: <skill_dir>
  env:         PATH, LANG
  sandbox:     enforcing (sandbox-exec)
```

## Eval harness integration

Once this protocol ships, `agentflow eval run` populates
`AssertionContext::skill_validator` from the case's resolved skill.
The `final_answer_matches_skill` variant works without further code
changes — the assertion layer already calls the closure.

In the eval report, a failed `final_answer_matches_skill` outcome
carries one of these reasons (verbatim):

- `"skill declares no validator"` — `kind = "none"` or no
  `[validation]`.
- `"skill validator rejected the final answer"` — exit ≠ 0 / regex
  mismatch.
- `"validator unrunnable: <stderr>"` — exit 125.
- `"validator timed out after Ns"` — timeout.

These map 1:1 to existing strings the assertion layer already
emits — no new variants in `AssertionOutcome.reason`.

## Compatibility

- Skills with no `[validation]` table: continue to work; the
  `final_answer_matches_skill` assertion fails for them (today's
  behaviour). No migration needed.
- Skills with custom `tests/validator.sh` scripts predating this
  protocol: add `[validation] kind = "command" command = ["bash",
  "tests/validator.sh"]` to opt in. The script itself doesn't need
  to change.
- The protocol is stable at first land. `schema_version = 1`.

## Stability tier

- `[validation]` manifest section, `kind` discriminator, the three
  v1 kinds (`none` / `regex` / `command`), and the command wire
  protocol (stdin → final_answer, exit code = verdict, 125 reserved):
  **stable** at first land.
- `SkillValidator` trait surface: **experimental** at first land,
  promote to stable after one outside crate consumes it.
- Future kinds (`json_shape`, `contains_all`, etc.) ship under
  `schema_version >= 2`; the loader rejects unknown kinds at v1.

## Related

- `docs/AGENT_EVAL_FORMAT.md` — the assertion that consumes this
  protocol.
- `docs/MCP_CAPABILITY_POLICY.md` — the precedence rules the command
  validator inherits when its executable goes through the
  `mcp_command_allowlist`.
- `agentflow-skills/src/manifest.rs` — where `SecurityConfig` lives;
  `[validation]` lands as a sibling section.
- `agentflow-tools/src/sandbox/` — the OS sandbox the command
  validator wraps when `security.os_sandbox = true`.

## Open questions (deferred to implementation)

1. **JSON output validators**: real agents often emit JSON. Should
   v1 include a `json_shape` kind out of the gate, or wait for the
   first user request? Current answer: wait — `regex` + `command`
   cover JSON shape via `jq -e` in a script.
2. **Per-case validator override**: should an `EvalCase` be able to
   override the skill's validator? Current answer: no — the case
   already has `expected_assertions[]` for case-local checks; the
   skill validator is meant to be a skill-wide acceptance contract.
3. **Validator running on partial answers**: should the validator
   see intermediate `AgentStep::FinalAnswer` candidates? Current
   answer: no — only the final string surfaced as
   `AgentRunResult::answer`.
