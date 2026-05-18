# L1 + L3 Integration Validation — Reflection R3

**Date**: 2026-05-18 (same day as R1 and R2, third reflection in the
sequence)
**Supersedes**: [R2](L1_L3_REFLECTION_R2_2026-05-18.md) for the action
queue. R2 stays as the authoritative source for the L1↔L3 selection
rule (unchanged by this round), the per-application matrix (A1 / A1.5
/ A7 / A2 / A3 / A2-follow-up), and the original 40-finding inventory.
R3 only synthesises the **sweep** that closed R2's open agentflow-side
items.
**Trigger**: After R2 froze the action queue, a single multi-hour
session worked the queue top-down and closed **all 8 agentflow-side
findings** (the only remaining open items are phonon-external or
low-priority docs). The patterns that emerged from that sweep are
worth recording before the next dogfooding pillar starts.

---

## 1. What got closed (8 commits, 4 crates touched, ~1700 LoC net)

| # | Commit | Finding | Crate | Surface |
| --- | --- | --- | --- | --- |
| 1 | `83a9765` | F-A2-9 | new `examples/applications/code-reviewer-write/` | Harness approval gate validation binary |
| 2 | `c552d3c` | F-A2-12 | `agentflow-harness` + `docs/HARNESS_MODE.md` | `HarnessProfile::Local` silent-auto-allow footgun docs |
| 3 | `9d386b3` | F-A2-11 | `agentflow-cli` + `agentflow-agents` | `harness run --approve` flag wires `HookedTool` |
| 4 | `bdaff36` | F-A7-4 | `agentflow-cli` doctor | `models.yml` source label with "overrides built-in" suffix |
| 5 | `9a96058` | F-A2-6 | `agentflow-cli` skill | `skill run --output json` single-object mode |
| 6 | `100c267` | F-AF-2 | `agentflow-skills` | SKILL.md frontmatter `model:` honoured (was dropped) |
| 7 | `d7651f7` | F-A2-13 | `agentflow-agents` | ReAct steering note on repeat tool calls |
| 8 | `0d921aa` | F-A2-5 | `examples/applications/code-reviewer/README.md` + `examples/README.md` | "LLM review is non-deterministic" practice docs |

**3 of 8 are pure-docs** (#2, #6 partial, #8), **4 of 8 are CLI/UX
visibility fixes** (#3, #4, #5, plus #2's docs aspect), **1 is a real
agent-runtime change** (#7), **1 is a new validation binary** (#1).
The skew toward docs + UX over runtime is consistent with R2's read
that "no findings require a core refactor" — the platform's
correctness is mostly there; the gaps are observability and reach.

## 2. Patterns that emerged

### 2.1 The Harness-from-CLI gap was a single line of missing wiring
F-A2-11 surfaced because `agentflow harness run` built a bare
`ReActAgent` and never called `wrap_registry(...)`. The fix added
`--approve {none|cli|auto-allow|auto-deny}` and a `ReActAgent::
with_tools` accessor; the actual diff that activates the approval gate
is ~15 lines in `harness/run.rs`. **Takeaway**: when the HTTP gateway
and CLI diverge on a foundational feature, the cost of porting is
often trivial — the cost is finding out. Dogfooding caught this where
the API documentation wouldn't.

### 2.2 Default-permissive can be more dangerous than no feature at all
F-A2-12 (`HarnessProfile::Local` silently auto-allows NonIdempotent
tools without an explicit pre-hook) was an existing design choice with
a clean rationale (local dev ergonomics) but no visible warning. The
fix was three rustdoc blocks + a footgun callout in
`docs/HARNESS_MODE.md` — zero code change. A first-time adopter
wiring `CliApprovalProvider::stdin()` expecting prompts to fire would
otherwise spend 15+ minutes reading `hooks_runtime.rs` to discover the
profile gating. **Takeaway**: opt-in security features need their
opt-in to be the loud thing in the docs, not the small print.

### 2.3 Silent serde drops are a recurring failure mode
F-AF-2 (`SKILL.md frontmatter model:` was dropped) had the same shape
as several earlier finds: a field the user reasonably expected to be
honoured was missing from the deserialisation struct, and serde's
default `deny_unknown_fields = false` swallowed it without complaint.
The fix is mechanically trivial (one struct field + one
`SkillManifest` assignment); the discovery cost is high because the
agent appears to be running normally with the wrong model. **Pattern
to watch**: any time a manifest schema is extended in skill.toml, the
SKILL.md path needs a matching extension OR an explicit
"unknown-field" warning. We may want a CI grep that flags drift.

### 2.4 Trace-clean steering beats hard blocks
F-A2-13's `ReActAgent` repeat-call detector was the only agent-runtime
change in the sweep. Two design choices made it land cleanly: (a) the
steering note goes into **memory only**, so trace replay stays
faithful to what the tool returned; (b) the tool **still runs both
times**, so legitimate retries (polling, idempotent re-reads) aren't
broken. The contrast with a hypothetical "block second identical
call" implementation is sharp — the latter would have needed a config
flag, a list of exempt tools, and probably a recovery path for false
positives. Advisory-by-default is the right default for soft
heuristics that touch the agent loop.

### 2.5 Source-of-truth labels close the "WTF is loading" gap
F-A7-4 didn't add a new piece of data — `models_config_source` was
already in the doctor JSON since before R1. What it added was a
human-readable label (`"/Users/.../models.yml (overrides built-in)"`)
plus a stable JSON enum kind (`"user_models_yml"`) plus prominent
positioning in the text output. The R2 finding was caught by grep —
the user saw the right info was hidden behind a Rust-debug-formatted
string. **Takeaway**: report-quality matters as much as report
coverage. The same data, surfaced badly, is the data the user can't
find.

### 2.6 Pure-docs commits closed 3/8 findings; their leverage is real
F-A2-5, F-A2-12, and (in part) F-AF-2 closed via docs without code.
Each of them encoded a lesson that took dogfooding time to learn the
first time and would have re-cost the next person the same time.
**Takeaway**: when a finding's value is "next person doesn't relearn
this", a docs commit is the highest-leverage close. Don't downgrade
docs work to "we should write that down sometime".

## 3. What's now true that wasn't before this sweep

- `agentflow harness run --approve cli --profile production` makes
  every NonIdempotent tool call (shell, file:write, mutating http)
  surface an interactive operator prompt before executing. CLI
  parity with the HTTP gateway, no hand-rolled binaries needed.
- `agentflow doctor` text output opens with the active `models.yml`
  source labelled `"~/.agentflow/models.yml (overrides built-in)"`
  or `"built-in default_models.yml"`. The JSON shape gains
  `models_config_source_kind` as a stable snake_case enum
  (`user_models_yml` / `user_models_yaml` / `env_override` /
  `built_in_default`).
- `agentflow skill run --output json` emits a single JSON object on
  stdout suitable for piping into jq or other tooling. Banners go
  to stderr; redaction still applies.
- SKILL.md frontmatter `model: <name>` actually takes effect (was
  silently dropped because the field wasn't on
  `SkillMdFrontmatter`).
- `HarnessProfile::Local` (default) carries explicit rustdoc warning
  about silent auto-allow; `docs/HARNESS_MODE.md` shipped snippet
  comments out the load-bearing `.with_profile(Production)` line so
  drive-by readers can't miss it.
- ReAct loop no longer burns through `MaxToolCalls` on the
  moonshot-v1-128k repeat-call pathology — second identical call
  carries an `[agentflow steering note (F-A2-13): ...]` in the
  memory message the model sees on its next turn.
- `examples/applications/code-reviewer/README.md` carries the
  concrete finding-set comparison from the two A2 dogfooding runs
  + practice guidance (3-5 runs and union, quorum for automated
  gates).
- New `examples/applications/code-reviewer-write/` binary
  demonstrates the end-to-end Harness approval flow with both
  `--auto-approve` (CI smoke) and `--prefetch-diff` (the
  workaround for F-A2-13's underlying model loop) modes.

## 4. What's left in the queue (15 items)

R2's count was 17 before the sweep started; this sweep closed 8 and
caught 1 new follow-up (F-A2-11 mentioned the moonshot-v1-128k
loop, which spawned F-A2-13 and was closed in the same session).
The remaining 15:

| Tier | Count | Nature |
| --- | --- | --- |
| Medium (phonon-external) | 5 | F-PH-1/2 etc. — not in the agentflow workspace; not gating anything here |
| Low (agentflow docs polish) | 4 | F-DOC-2/3/4, F-A7-5 — small docstring / inline-comment touches |
| Low (LLM tooling polish) | 3 | F-A7-6 (`llm models --refresh-from-api`), F-A7-7 (dotenvy helper), F-AF-4 (Moonshot/Anthropic init error message) |
| Low (examples convention) | 2 | F-EX-1 (A1.5 persona LUFS verify), F-PH-3 (phonon-mcp `audio_info.resampled_from`) |
| Low (sandboxing) | 1 | (none currently flagged) |

None of these are dogfooding blockers. None require core changes.
The natural next dogfooding pillar is **building a new application
that exercises an un-validated platform surface** rather than
draining the docs queue further.

## 5. Recommended next pillar: A6 doc-translator

R2's matrix shows we have empirical evidence for L1 fixed pipelines
(A1, A7, A3), L3 agent decisions (A1.5, A2), and L1-binary-wrapping-
Harness (A2 follow-up). **The biggest un-validated DAG primitive is
`map` parallel execution under load**, and the natural application
shape for it is A6 (doc-translator) per the existing TODOs entry.

A6 would validate:
- `map` parallel node over (files × target languages) — fan-out 100+
- LLM rate-limit handling under that fan-out
- Per-file failure isolation (one file failing shouldn't tank the run)
- Checkpoint recovery (mid-run failure → resume from where we stopped,
  don't re-translate)
- File batch write coordination

The platform's `agentflow-core::Flow::execute` already has the
`Concurrent` mode and `max_concurrency` knob; A6 would be the first
real load test. Of the alternatives:
- A4 (meeting-transcriber) requires an ASR API; out-of-pocket cost.
- A5 (weekly-digest) requires SMTP and reuses A3's RAG store; smaller
  validation surface than A6.

## 6. What this round did NOT change

- The L1↔L3 selection rule (pass-through → L1, agent-decides → L3)
  stands unchanged. No new evidence either falsified it or refined it.
- Phonon scope reflection: no new evidence to bump the 4 maybe-archive
  crates (studio / video / wasm / plugin). Decision unchanged.
- Performance baselines: no new data points worth adding to R2's
  table; this sweep was UX work, not runtime work (with the
  exception of F-A2-13 which adds negligible overhead — one
  `Value::eq` per iteration).

R3 is therefore mostly a delta on R2's "action queue" and "what's
now true" sections. The structural conclusions of R1 and R2 carry
forward.

---

**Recommended action queue for the next session**: pick A6 or end
the dogfooding phase here and shift to v0.3.0 release prep. The
R2-originated items don't have anything left that's both small and
high-value; the remaining L-priority items can be batched as a
single "docs sweep" closer to release if needed.

---

## Addendum (2026-05-18, same session)

After R3 landed, A6 doc-translator iteration 1 shipped
(commit `141b993`) and surfaced 4 findings. The two most-blocking
were closed in the next commit:

- **F-A6-1** — `map parallel` had no concurrency cap. Closed:
  `NodeType::Map` gained `max_concurrent: Option<usize>`,
  `execute_map_node_parallel` now uses `tokio::sync::Semaphore`
  per-sub-flow. Unbounded behaviour preserved for `None`
  (back-compat). `Some(0)` rejected as config error rather than
  deadlocking. Two new unit tests in `agentflow-core` assert the
  cap holds and zero is rejected. Live A6 re-run with
  `max_concurrent: 3` on N=4 inputs: 4/4 OK (was 3/4 before).
- **F-A6-2** — `workflow validate` warned on undeclared map
  fields. Closed: ParamSpec list bumped to include `input_list`
  and `max_concurrent`. Validate now passes clean.

F-A6-3 (per-sub-flow Err buried in nested results) and F-A6-4
(prompt ambiguity for self-translation) remain open as iter 2
follow-ups. Neither blocks A6 scaling up.
