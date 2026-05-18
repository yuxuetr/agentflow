# A7 — changelog-writer

**Status**: WIP — live end-to-end ✅ (2026-05-18 first run on
`v0.2.0..HEAD`, 399 commits, ~2min wall clock).
**Tracking entry**: [`EXAMPLES_TODOs.md` § A7](../../../EXAMPLES_TODOs.md#a7--changelog-writer)

## Business

Input: a git tag range string (e.g. `v0.2.0..HEAD`).
Output: a markdown changelog grouped by Conventional Commits type
(`feat` / `fix` / `perf` / `refactor` / `docs` / `test` / `ci` /
`chore` / `style` / `revert` / `other`), suitable for pasting into
release notes.

## Architecture: L1 binary (NOT a skill)

Two AgentFlow nodes in a Flow:

```
┌──────────────┐    raw git log    ┌──────────────────┐    markdown
│ RunGitLogNode│ ────────────────▶ │ ClassifyAndRender│ ────────▶  stdout / file
│ (subprocess) │                   │ (single LLM call)│
└──────────────┘                   └──────────────────┘
```

1. **`RunGitLogNode`** — spawns `git log <range> --no-merges
   --pretty=format:'%h|||%s|||%b%n===COMMIT==='` via `std::process`.
   No agent. The range string flows through `initial_inputs` as a
   `FlowValue::Json(String)` and is passed verbatim to git.
2. **`ClassifyAndRenderNode`** — one `LlmInit::model(...).prompt(...).execute()`
   call (default `moonshot-v1-128k`). The full raw git log goes
   into the prompt; the LLM returns categorized markdown. No tool
   calling, no ReAct loop, no agent decisions.

## Why a binary, not a skill (decision log)

The original A7 spec called for a skill that drives an agent to
shell out to git. That **was attempted and rejected** — see
[`skill.toml.rejected`](skill.toml.rejected) next to this README for
the full original manifest, kept as documentation.

Across multiple model attempts (`moonshot-v1-128k`, `kimi-k2.6`) the
ReAct agent consistently substituted the user-provided range with
hallucinated "typical example" ranges (`v1.0.0..v1.1.0`,
`v1.0.0..v2.0.0`, `v1.2.3..v1.3.0`) — even when given a real existing
tag (`v0.2.0..HEAD`). The model's "be helpful, correct the input"
behaviour overrode the persona's "use the user's exact string"
instructions in every variant tried.

This is an instance of the L1+L3 reflection rule, validated under
fire:

> **Fixed pipeline → L1** (no LLM-in-loop tax).
> **Agent picks tools / branches → L3** (LLM-in-loop tax unavoidable).

Changelog generation is a **fixed pipeline** (one shell call → one
LLM call → write output) and the inputs (range string) require
**verbatim pass-through**. Both arrows point to L1.

The skill form would be appropriate if the agent had real decisions
to make (which range? which commits to include? which categorization
scheme?). For this app's actual shape, the binary is correct and
the skill form was fighting the architecture.

## External dependencies

| Dep | How to satisfy |
| --- | --- |
| `git` on PATH | Standard system install. |
| LLM API key | Default model is `moonshot-v1-128k`; needs `MOONSHOT_API_KEY`. Auto-loaded from `~/.agentflow/.env` (via P9.3). Override model with `--model <name>` if you prefer another agentflow-llm provider. |

## Files

```
changelog-writer/
├── README.md                # ← this file
├── Cargo.toml               # standalone Cargo project; path deps to
│                            # agentflow-core + agentflow-llm
├── src/
│   └── main.rs              # RunGitLogNode + ClassifyAndRenderNode +
│                            # 2-node Flow + CLI parse
└── skill.toml.rejected      # original L3 skill attempt; preserved as
                             # documentation of why L1 is correct here
```

## Run

```bash
cd examples/applications/changelog-writer
# MOONSHOT_API_KEY auto-loaded from ~/.agentflow/.env

# Write to a file:
cargo run --release -- \
  --range v0.2.0..HEAD \
  --output /tmp/CHANGELOG-v0.2.0-to-HEAD.md

# Or print to stdout (suitable for piping):
cargo run --release -- --range v0.2.0..HEAD > /tmp/CHANGELOG.md

# Override model:
cargo run --release -- \
  --range v0.2.0..HEAD \
  --model moonshot-v1-32k
```

## First-run observations (2026-05-18)

- **End-to-end wall clock**: ~117s for `v0.2.0..HEAD` (399 commits,
  354 KB raw git log → 11 KB markdown).
- **Per-node**: `git_log` 38ms, `classify_render` 116.6s (all of it
  is the single LLM call to moonshot-v1-128k with 354k chars of input
  context).
- **Output quality**: model added GitHub commit URL links beyond the
  prompt's spec — graceful "do more than asked" rather than "ignore
  the spec". Bullets render cleanly, categorization respects scope
  parenthesis (e.g. `feat(cli):` stays grouped under Features).
- **Truncation**: 4096 max_tokens (Moonshot default in agentflow's
  models.yml) capped the output mid-hash on the 119th line. See
  [A7 Findings finding #18](../../../EXAMPLES_TODOs.md#a7--changelog-writer)
  — for ranges with > ~100 commits, bump `max_tokens` in models.yml
  or split per-category.

## What this validates in AgentFlow

- Multi-node `Flow` with custom AsyncNode wrapping
  `std::process::Command::new("git")` — proves the "shell out from
  inside a node" pattern (cheaper than building a generic shell
  workflow node).
- `LlmInit::model(...).prompt(...).execute()` as a one-shot
  programmatic LLM call (the fluent surface inside a custom
  AsyncNode). No ReAct loop, no Tool registry — just LLM-as-function.
- `initial_inputs` field on a `GraphNode` for threading literal
  values into the entry node (the range string here).
- The dogfooding rule "fixed pipeline → L1, agent decides → L3"
  was tested under fire; L1 won, skill form rejected with evidence.

## Findings during dogfooding

See [`EXAMPLES_TODOs.md` § A7 Findings](../../../EXAMPLES_TODOs.md#a7--changelog-writer)
for the live list (8 new findings this run, including the
skill-form rejection rationale, the LLM input substitution failure
mode, agentflow-llm registry lag behind Moonshot's `/v1/models`, the
config/models/*.yml vs templates/default_models.yml lookup
precedence surprise, and the kimi-k2.6 mandatory `temperature=1.0`).
