# Release Checklist

Use this checklist before tagging or publishing an AgentFlow release. The
automated checks are enforced by the `release gate` job in
`.github/workflows/quality.yml`; the remaining items require human judgement.

For release tracking, open `.github/ISSUE_TEMPLATE/release.md` and link the CI
run that passed on the release branch or tag.

## CI Coverage

The `release gate` job requires these automated jobs to pass:

- `cargo fmt`
- `cargo clippy`
- package test matrix for `agentflow-core`, `agentflow-tools`,
  `agentflow-memory`, `agentflow-mcp`, `agentflow-skills`,
  `agentflow-agents`, and `agentflow-cli`
- workspace doc tests
- selected feature combinations
- workspace examples compile and no-API smoke tests

The checklist below keeps local parity commands for reproducing CI failures,
then lists the manual release review items.

## 1. Scope

- [ ] Confirm release target, version, and branch.
- [ ] Review `RoadMap.md`, `TODO.md`, and release notes for completed work.
- [ ] Confirm ignored local files such as `TODOs.md`, `target/`, and run output
      directories are not required in the release commit.
- [ ] Review `git status --short --ignored` and ensure only intentional ignored
      files remain.

## 2. Code Quality (CI Covered)

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target-dir /tmp/agentflow-target
cargo test --workspace --doc --target-dir /tmp/agentflow-target
```

- [ ] Formatting passes.
- [ ] Clippy has no warnings.
- [ ] Workspace compile check passes.
- [ ] Workspace doc tests pass.
- [ ] No accidental debug prints, temporary fixtures, or local paths are present.

## 3. Core Test Matrix (CI Covered)

Run the focused crate matrix first so failures are easy to localize:

```bash
cargo test -p agentflow-core --target-dir /tmp/agentflow-target
cargo test -p agentflow-tools --target-dir /tmp/agentflow-target
cargo test -p agentflow-memory --target-dir /tmp/agentflow-target
cargo test -p agentflow-mcp --target-dir /tmp/agentflow-target
cargo test -p agentflow-skills --target-dir /tmp/agentflow-target
cargo test -p agentflow-agents --target-dir /tmp/agentflow-target
cargo test -p agentflow-cli --target-dir /tmp/agentflow-target
```

Then run the full workspace:

```bash
cargo test --workspace --target-dir /tmp/agentflow-target
```

- [ ] Core workflow/checkpoint tests pass.
- [ ] Tool registry and built-in tool tests pass.
- [ ] MCP protocol, client, and integration tests pass.
- [ ] Skill manifest compatibility and MCP skill tests pass.
- [ ] Agent runtime unit and golden tests pass.
- [ ] CLI tests pass.
- [ ] Full workspace tests pass, or documented exclusions are approved.

## 4. Feature Matrix (CI Covered)

The CI feature matrix intentionally checks selected combinations instead of
`--all-features`, because some optional integrations are designed for external
services or heavier local runtimes.

Current feature inventory:

- `agentflow-core`: `observability`.
- `agentflow-mcp`: `client`, `server`, `stdio`, `http`.
- `agentflow-cli`: `mcp`, `rag`.
- `agentflow-llm`: `openai`, `anthropic`, `google`, `observability`, `logging`.
- `agentflow-nodes`: `llm`, `http`, `file`, `template`, `batch`,
  `conditional`, `factories`, `mcp`, `rag`.
- `agentflow-rag`: `qdrant`, `local-embeddings`, `pdf`, `html`.
- `agentflow-tracing`: `postgres`.
- `agentflow-agents` and `agentflow-viz`: empty default feature sets.

CI-covered combinations:

```bash
cargo check -p agentflow-core --features observability --target-dir /tmp/agentflow-target
cargo check -p agentflow-mcp --features client,server,stdio --target-dir /tmp/agentflow-target
cargo check -p agentflow-cli --no-default-features --features mcp --target-dir /tmp/agentflow-target
cargo check -p agentflow-cli --no-default-features --features rag --target-dir /tmp/agentflow-target
```

- [ ] CI-covered feature combinations pass.
- [ ] Any skipped feature combination is documented with service/runtime
      requirements.

## 5. Integration Smoke Tests (CI Covered)

```bash
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo test --workspace --examples --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-core --example fixed_dag_workflow --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example agent_native_react --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example plan_execute_agent --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- workflow run agentflow-cli/examples/workflows/fixed_dag_basic.yml --dry-run
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- workflow run agentflow-cli/examples/workflows/skill_agent_hybrid.yml --dry-run
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --features rag --target-dir /tmp/agentflow-target -- workflow run agentflow-cli/examples/workflows/rag_skill_assistant.yml --dry-run
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index validate agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index list agentflow-skills/examples/skills.index.toml
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill index resolve agentflow-skills/examples/skills.index.toml mcp-demo
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill validate agentflow-skills/examples/skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-cli --target-dir /tmp/agentflow-target -- skill list-tools agentflow-skills/examples/skills/mcp-basic
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-skills --example skill_calls_mcp_tool --target-dir /tmp/agentflow-target
HOME=/tmp/agentflow-home CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup \
  cargo run -p agentflow-agents --example hybrid_workflow_agent --target-dir /tmp/agentflow-target
```

- [ ] Workspace examples compile.
- [ ] Fixed DAG example runs.
- [ ] Agent-native ReAct mock example runs.
- [ ] Plan-and-Execute mock example runs.
- [ ] Config-first fixed DAG, Skill-agent, and RAG + Skill dry-runs pass.
- [ ] Skill registry index validates, lists, and resolves.
- [ ] DAG + Agent hybrid example runs.
- [ ] Skill examples validate and list tools through the CLI.
- [ ] A local MCP skill can validate, list tools, and run a tool call.

## 6. Runtime Contracts (Manual)

- [ ] `AgentRunResult` golden fixture changes are intentional and reviewed.
- [ ] Workflow checkpoint resume still skips completed nodes.
- [ ] `AgentNode` output includes `response`, `session_id`, `stop_reason`, and
      `agent_result`.
- [ ] `WorkflowTool` exposes schema and timeout behavior as documented.
- [ ] Trace output links workflow, agent, tool, and MCP calls where applicable.

## 7. Documentation (Manual)

- [ ] `README.md` reflects the current release positioning.
- [ ] `docs/AGENT_RUNTIME.md` matches runtime behavior.
- [ ] `docs/SKILL_FORMAT.md` matches `SKILL.md` / `skill.toml` behavior.
- [ ] MCP skill docs cover server naming, tool naming, and error behavior.
- [ ] Release notes include breaking changes, migration notes, and known issues.
- [ ] Examples referenced by docs compile or run.

## 8. Version And Packaging (Manual)

- [ ] Crate versions are updated consistently.
- [ ] `Cargo.lock` is intentionally updated.
- [ ] Feature flags and default features are reviewed.
- [ ] `cargo package --list` for publishable crates excludes local artifacts.
- [ ] Release notes and tag name match the version.

## 9. Final Gate (Manual)

```bash
git diff --check
git status --short --ignored
```

- [ ] No whitespace errors.
- [ ] No unintended tracked changes.
- [ ] `release gate` passed on the release branch or tag.
- [ ] Release commit is tagged only after all required checks pass.
- [ ] Any skipped check is recorded in the release notes with owner and reason.
