# CI Workflows

This document is the source of truth for what runs in GitHub Actions for
the AgentFlow repository, what each job gates, and which planned jobs do
not yet exist. It is intentionally short: read the workflow YAML for the
ground truth, read this doc for the *map*.

Last audited: 2026-05-14.

## Existing workflows

| File | Trigger | Required for release gate | Purpose |
|------|---------|---------------------------|---------|
| `.github/workflows/quality.yml` | `push` to `main`/`master`, `pull_request`, `workflow_dispatch`, tag `v*` | Yes (`release-gate` job) | Format / clippy / per-crate tests / doc tests / curated feature checks / example smoke. |
| `.github/workflows/llm-live.yml` | Daily cron (09:30 UTC) + `workflow_dispatch` | No — non-deterministic, costs money | Cross-provider live LLM smoke against the real OpenAI / Anthropic / Google / Moonshot / StepFun endpoints. |

### `quality.yml` jobs

| Job | What it gates |
|-----|---------------|
| `fmt` | `cargo fmt --all --check`. |
| `clippy` | `cargo clippy --workspace --all-targets -- -D warnings`. |
| `test` | `cargo test -p <pkg> --all-targets` per matrixed crate (`agentflow-core`, `agentflow-tools`, `agentflow-mcp`, `agentflow-memory`, `agentflow-skills`, `agentflow-agents`, `agentflow-cli`). Crates not in the matrix (e.g. `agentflow-db`, `agentflow-server`, `agentflow-worker`, `agentflow-ui`, `agentflow-rag`, `agentflow-tracing`, `agentflow-viz`, `agentflow-llm`) currently rely on `cargo build --workspace` plus their dependents' tests. Expanding this matrix is part of M.3. |
| `doctest` | `cargo test --workspace --doc`. |
| `features` | Hand-picked feature combinations (`agentflow-core --features observability`, `agentflow-mcp --features client,server,stdio`, `agentflow-cli --no-default-features --features mcp\|rag\|plugin`, `agentflow-core --features plugin --all-targets`). The exhaustive matrix is tracked under P3.9 (`feature-matrix.yml`, not yet created). |
| `examples` | Compiles all workspace examples and runs the no-API smoke set (fixed DAG, ReAct, Plan-Execute, skill index / validate / list-tools, hybrid agent, plugin host, plugin marketplace install). |
| `release-gate` | Aggregate `success` check across all six jobs above. PR-blocking. |

### `llm-live.yml` notes

- Each provider test self-skips with a log line when its API key secret is
  empty, so the suite is safe to flip on a single provider at a time
  without rewriting the matrix.
- The job has a 15-minute wall-clock timeout; individual tests carry their
  own 30s timeout in code.
- **Flake profile:** non-deterministic by design (provider behaviour
  shifts with model versions and load). Treat alert noise as informative,
  not blocking. Never gate releases on this workflow.

## Planned workflows (not yet created)

These exist as TODOs in `TODOs.md`; the file column below is the path the
tracking task expects the new workflow to live at.

| TODO ID | File | Purpose |
|---------|------|---------|
| P3.9    | `.github/workflows/feature-matrix.yml` | Exhaustive `cargo check` matrix across feature combinations (`--no-default-features` + each feature, `--all-features`). Marks broken combinations explicitly. |
| P3.10   | `.github/workflows/examples-smoke.yml` | Runs every example in `examples/ecosystem/` and `agentflow-*/examples/` with mock providers under a 5-minute cap; fails CI on any error or panic. |
| P4.1    | `.github/workflows/rag-eval-smoke.yml` | Runs `agentflow rag eval` against `eval_datasets/ci_offline/` and asserts schema (`recall@5`, `mrr`, `ndcg@10`, `latency_ms_p50/p95`). |
| P7.2    | `.github/workflows/bench.yml` | Runs `cargo bench` on a fixed runner, compares against `benches/baselines/<host>.json`, fails when median ≥ 1.25× baseline. Posts a PR summary. |

When any of these workflows is created, update this table accordingly and
add an entry to the "Existing workflows" table above.

## Flake-prone jobs and retry policy

| Job | Risk | Mitigation |
|-----|------|------------|
| `quality.examples` | The plugin-host demo spawns a subprocess; if `cargo build` produces an artifact that fails to spawn on the runner (rare), the step errors. | No automatic retry. Re-run the job on transient failure; investigate if it recurs. |
| `llm-live.live-smoke` | Provider 429s, model deprecations, slow nights. | Per-test 30s timeout in code, 15-minute wall-clock on the job. No retry — flake here is signal, not noise. |
| `quality.test` (per crate) | A small number of tests touch the filesystem (`agentflow-tools` sandbox matrix, `agentflow-skills` install fixtures) and can collide if the runner cache is dirty. | None today. If observed, isolate via `--test-threads=1` for the offending crate. |

Other jobs (`fmt`, `clippy`, `doctest`, `features`) are deterministic and
should not flake. Any flake there is a real bug.

## Conventions

- New gating jobs go into `quality.yml` so they participate in the
  aggregate `release-gate` check. Non-blocking jobs (cost / live API /
  perf) live in their own workflow file and are intentionally excluded
  from the gate.
- Job names use lower-case kebab-case (`feature-matrix`, `rag-eval-smoke`).
- Workflows write their purpose and gating intent in a comment block at
  the top of the file, like `llm-live.yml` does. New workflow files
  should follow that pattern so a reader doesn't need to cross-reference
  this doc.
