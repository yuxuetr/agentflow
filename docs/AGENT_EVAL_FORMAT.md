# Agent Eval Format

Status: design as of `P4.3`.
Crate (forthcoming impl): `agentflow-agents::eval` + CLI
`agentflow eval run`.
Implements: P4.4 (runner + CLI).

The agent eval harness measures *whether the agent produced the right
answer with acceptable cost*. It is intentionally not a unit test
framework — eval cases describe end-to-end behaviour ("given this
prompt, the agent must call the `web_search` tool at least once and
the final answer must mention 'OAuth'") and run a live agent loop
end to end against the dataset.

It is a sibling of the existing RAG eval harness
(`agentflow-rag::eval`) and reuses its on-disk style (JSONL + a
small TOML manifest) so the two halves of "is the AgentFlow stack
working?" feel like one tool to operators.

## When to use

- Release gate: pin agent behaviour before tagging a version. P4.4
  blocks release-gate quality claims on a green eval run.
- Regression catch: a refactor of `ReActAgent`, the reflection
  strategy, or memory layering should not silently change which
  tools the agent picks.
- Skill verification: a skill author runs `agentflow eval run` on
  the skill's bundled fixtures before publishing.

It is **not** suitable for:

- Provider quality benchmarks (`docs/LLM_PROVIDERS_MATRIX.md` and
  `P3.6` are the right surfaces).
- Retrieval quality (`docs/RAG_EVAL.md`).
- Latency profiling under load (`P7.1` benches).

## Dataset layout

One dataset = one directory.

```
my_eval_dataset/
  dataset.toml           # name, version, source, license, default limits
  cases.jsonl            # one EvalCase per line
  fixtures/              # optional: files referenced by cases
    invoice_001.pdf
    customer_export.csv
```

The format intentionally mirrors `agentflow-rag/eval_datasets/`:
JSONL keeps the file diff-friendly and append-friendly, and a
manifest TOML lets the dataset declare defaults that every case
inherits.

### `dataset.toml`

```toml
schema_version = 1
name = "rust-expert-smoke"
version = "0.1.0"
description = "End-to-end smoke for the rust_expert skill."
source = "internal"
license = "CC0-1.0"

# Per-case defaults. Any case can override.
[defaults]
skill = "examples/skills/rust_expert"
max_steps = 12
max_tool_calls = 6
cost_limit_usd = 0.50
latency_limit_ms = 60000
model = "mock-model"
```

`defaults` is optional. When omitted, every case must specify the
fields explicitly.

### `cases.jsonl`

One `EvalCase` per line:

```jsonc
{
  "id": "rust-expert-001",
  "prompt": "Review this unsafe block: `unsafe { ptr.read() }`",
  "skill": "examples/skills/rust_expert",          // override
  "tools_allowed": ["shell", "file"],              // narrows admission
  "max_steps": 8,
  "max_tool_calls": 4,
  "cost_limit_usd": 0.10,
  "latency_limit_ms": 30000,
  "model": "mock-model",
  "inputs": { "code_path": "fixtures/sample.rs" }, // optional, injected
  "expected_assertions": [
    { "type": "contains", "needle": "ptr.read" },
    { "type": "tool_called", "tool": "file", "min_count": 1 },
    { "type": "tool_not_called", "tool": "shell" },
    { "type": "step_count_below", "max_steps": 6 }
  ],
  "notes": "Reproduces the 2026-04 customer report."
}
```

### `EvalCase` fields

| Field | Type | Required | Default source | Notes |
| --- | --- | --- | --- | --- |
| `id` | string | yes | — | Stable identifier; appears in reports + trace IDs. |
| `prompt` | string | yes | — | The user input that opens the run. |
| `skill` | string | conditional | `defaults.skill` | Path to the skill dir or registry name. |
| `tools_allowed` | string[] | no | inherit from skill | Same precedence as `--allow-tool` CLI flag (P3.5). Narrows the registry the runtime sees. |
| `tools_denied` | string[] | no | `[]` | Mirrors `--deny-tool` for explicit refusal. |
| `inputs` | object | no | `{}` | Top-level key/value injected into the agent's initial state. |
| `max_steps` | int | no | `defaults.max_steps` → `RuntimeLimits::default().max_steps` (15) | Maps directly to `RuntimeLimits::max_steps`. |
| `max_tool_calls` | int | no | `defaults.max_tool_calls` | Maps to `RuntimeLimits::max_tool_calls`. |
| `cost_limit_usd` | float | no | `defaults.cost_limit_usd` | Hard cap on accumulated provider cost. Run fails with `CostLimitExceeded` when crossed. |
| `latency_limit_ms` | int | no | `defaults.latency_limit_ms` | Maps to `RuntimeLimits::timeout_ms`. |
| `model` | string | no | `defaults.model` or skill manifest | Provider model id; honours the same lookup as `agentflow workflow run --model`. |
| `expected_assertions` | array | yes | — | See below. Empty array is a hard error (always-pass cases are a mistake). |
| `notes` | string | no | — | Free-form human note. Surfaced in failure reports. |

`schema_version` (top-level inside each JSONL line) is optional and
defaults to `1`. Future incompatible changes bump this.

## Assertion DSL

Each entry in `expected_assertions` is a tagged JSON object. Six
variants today; the closed set is part of the v1 stability promise.

### `contains`

```jsonc
{ "type": "contains", "needle": "OAuth", "in": "final_answer" }
```

| Field | Default | Meaning |
| --- | --- | --- |
| `needle` | — | Substring to find (case-sensitive). |
| `in` | `"final_answer"` | Where to search: `final_answer` / `any_step` / `any_tool_result`. |
| `case_insensitive` | `false` | Lowercase both sides before matching. |

### `regex`

```jsonc
{ "type": "regex", "pattern": "(?i)\\boauth\\s+token\\b", "in": "final_answer" }
```

| Field | Default | Meaning |
| --- | --- | --- |
| `pattern` | — | Rust-flavour regex (`regex` crate). |
| `in` | `"final_answer"` | Same options as `contains`. |

### `tool_called`

```jsonc
{ "type": "tool_called", "tool": "web_search", "min_count": 1, "max_count": 3 }
```

| Field | Default | Meaning |
| --- | --- | --- |
| `tool` | — | Tool name as registered in the agent's `ToolRegistry`. |
| `min_count` | `1` | Minimum invocations. |
| `max_count` | `usize::MAX` | Maximum invocations. Use to forbid retries past N. |
| `with_params` | absent | Optional JSON object subset — every key must match the call's params. |

### `tool_not_called`

```jsonc
{ "type": "tool_not_called", "tool": "shell" }
```

Strict variant of `tool_called` with `max_count = 0`. Surfaced
separately because the typical failure mode is "agent called the
forbidden tool" and the report message reads better when the
assertion type names it.

### `step_count_below`

```jsonc
{ "type": "step_count_below", "max_steps": 8 }
```

Asserts the run terminated with strictly fewer than `max_steps`
`AgentStep`s. Useful when a refactor accidentally inflates the loop
without hitting `RuntimeLimits::max_steps`.

### `final_answer_matches_skill`

```jsonc
{ "type": "final_answer_matches_skill" }
```

Runs the skill's bundled fixture verifier (`tests/smoke.sh` or the
skill manifest's `[validation]` section) against the final answer.
Lets skill authors keep a single authoritative pass/fail function
without duplicating it into every eval case.

If the skill has no validator declared, this assertion fails with
`AssertionUnsupported { reason: "skill declares no validator" }`.

## Output schema

`agentflow eval run <dataset>` produces one report per invocation.
Default output is human-readable; `--format json` emits the
machine-readable envelope.

### JSON envelope

```jsonc
{
  "schema_version": 1,
  "dataset": "rust-expert-smoke",
  "dataset_version": "0.1.0",
  "started_at": "2026-05-15T10:12:00Z",
  "finished_at": "2026-05-15T10:13:42Z",
  "summary": {
    "total": 12,
    "passed": 11,
    "failed": 1,
    "skipped": 0,
    "cost_usd_total": 0.83,
    "latency_ms_p50": 4200,
    "latency_ms_p95": 12100
  },
  "cases": [
    {
      "id": "rust-expert-001",
      "status": "passed",
      "trace_id": "01HZ3K…",
      "started_at": "…",
      "finished_at": "…",
      "duration_ms": 3812,
      "cost_usd_actual": 0.04,
      "stop_reason": "final_answer",
      "step_count": 5,
      "tool_call_count": 2,
      "assertions": [
        { "type": "contains", "passed": true },
        { "type": "tool_called", "tool": "file", "passed": true, "actual_count": 2 }
      ]
    },
    {
      "id": "rust-expert-006",
      "status": "failed",
      "trace_id": "01HZ3K…",
      "duration_ms": 21034,
      "cost_usd_actual": 0.12,
      "stop_reason": "max_tool_calls",
      "step_count": 9,
      "tool_call_count": 6,
      "assertions": [
        {
          "type": "tool_not_called",
          "tool": "shell",
          "passed": false,
          "actual_count": 1,
          "reason": "shell called once at step 4"
        }
      ],
      "notes": "Agent fell back to shell after web_search 503'd; check VCR fixture."
    }
  ]
}
```

### Status values

| Status | When |
| --- | --- |
| `passed` | Every assertion passed AND `stop_reason.is_success()`. |
| `failed` | One or more assertions failed, OR the run terminated with a non-success stop reason (`MaxSteps`, `MaxToolCalls`, `Timeout`, `TokenBudgetExceeded`, `CostLimitExceeded`, `Error`, `Cancelled`). |
| `skipped` | Case was filtered out (`--filter`, `--skill`, manifest gate, etc.). Skipped cases do not count toward pass/fail. |

`stop_reason` values map 1:1 to `AgentStopReason` variants (see
[`agentflow-agents/src/runtime.rs`](../agentflow-agents/src/runtime.rs))
plus the new `CostLimitExceeded` value introduced by the eval
harness when `cost_limit_usd` is crossed.

## Cross-reference with trace replay

Every failed case carries a `trace_id` that resolves through the
existing `agentflow trace replay` machinery:

```bash
agentflow eval run my_dataset --format json > report.json
# Pick a failed case from the report:
jq -r '.cases[] | select(.status=="failed") | .trace_id' report.json | \
  xargs -n1 agentflow trace replay
```

The eval runner writes traces under the same `AGENTFLOW_TRACE_DIR`
the rest of the stack uses, so `trace replay` works without any
extra flag plumbing. For TUI debugging, swap `replay` for `tui`.

## Reusing Flow as the eval pipeline

The runner is internally a `Flow` (`agentflow-core::Flow`) with one
node per case. This is a deliberate choice:

- The concurrency budget is the same as everywhere else: `Flow`'s
  `FlowExecutionMode::Concurrent` with `max_concurrency = N` runs
  cases in parallel.
- Checkpoints land in the normal `~/.agentflow/checkpoints/<run_id>`
  tree so a long eval can be resumed via `agentflow workflow
  resume-plan <eval_run_id>`.
- Trace events emit through the existing `EventListener` chain, so
  `--otel` flags Just Work.
- The `Flow` itself can be inspected with `agentflow workflow validate`.

Concretely, the runner constructs:

```
Flow {
  nodes: [
    EvalCaseNode { id: "rust-expert-001", ... },
    EvalCaseNode { id: "rust-expert-002", ... },
    …
  ],
  mode: Concurrent { max_concurrency: cli.parallelism },
}
```

`EvalCaseNode::execute` builds a `ReActAgent` (or the runtime named
in `dataset.toml`), threads `RuntimeLimits` from the case fields,
runs the agent to terminal state, evaluates assertions against the
captured `AgentStep`s + final answer, and emits a per-case JSON row.

## CLI surface (lands under P4.4)

```bash
# Run a dataset and print a human-readable summary
agentflow eval run path/to/dataset

# Machine-readable JSON envelope
agentflow eval run path/to/dataset --format json --output report.json

# Filter by case id glob
agentflow eval run path/to/dataset --filter "rust-expert-*"

# Force a parallelism for local debugging
agentflow eval run path/to/dataset --parallelism 1

# Treat eval failure as a hard CI signal
agentflow eval run path/to/dataset --fail-on-status failed

# Compare against a checked-in baseline (mirrors `rag eval --compare-baseline`)
agentflow eval run path/to/dataset --compare-baseline baselines/main.json
```

Exit codes:

- `0` — every case `passed` (or `skipped`).
- `1` — one or more cases `failed`, but the dataset shape was valid.
- `2` — dataset or assertion config was invalid; no agent run was
  attempted.

## Determinism and the mock provider

CI runs the eval harness against the `mock` provider by default. The
mock provider replays canned responses, so a green eval is a strict
contract that the agent loop, memory layer, and tool dispatcher
haven't drifted. Live provider runs are opt-in via
`AGENTFLOW_LIVE_PROVIDER=1` and gated on the relevant API key —
matching the P3.6 pattern.

Reproducibility checklist for case authors:

- Pin the model id (`model = "mock-model"` in `dataset.toml`
  `[defaults]`).
- Seed any randomness in tools (e.g. `web_search` mocks should
  honour `AGENTFLOW_MOCK_SEED`).
- Provide every external input through `inputs` or `fixtures/`,
  never via env vars.
- Avoid `Timeout`-based assertions for non-mock runs; use
  `latency_limit_ms` only for budget enforcement, not for behaviour.

## Stability

- `EvalCase` JSON fields, `dataset.toml` `[defaults]` keys: **stable**
  at first land under P4.4.
- Assertion DSL closed enum of six variants: **stable** at first land.
  Future variants must come through a `schema_version` bump.
- JSON report envelope shape: **stable**; additive fields are
  tolerated per the P0.3 stability tier.

See `docs/STABILITY.md` for tier definitions.

## Related

- `docs/RAG_EVAL.md` — the sibling harness this format mirrors.
- `docs/AGENT_RUNTIME.md` — `AgentStep`, `AgentStopReason`,
  `RuntimeLimits` reference.
- `docs/STABILITY.md` — what "stable" means for the assertion DSL.
- [`agentflow-agents/src/runtime.rs`](../agentflow-agents/src/runtime.rs) —
  source of truth for `AgentStopReason` and `RuntimeLimits`.
- [`agentflow-rag/src/eval/`](../agentflow-rag/src/eval/) — implementation
  reference for the JSONL + manifest + runner pattern.
