# Release Checklist

Use this checklist before tagging or publishing an AgentFlow release. It is the
manual quality gate until the same checks are fully mirrored in CI.

## 1. Scope

- [ ] Confirm release target, version, and branch.
- [ ] Review `RoadMap.md`, `TODO.md`, and release notes for completed work.
- [ ] Confirm ignored local files such as `TODOs.md`, `target/`, and run output
      directories are not required in the release commit.
- [ ] Review `git status --short --ignored` and ensure only intentional ignored
      files remain.

## 2. Code Quality

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check --workspace --target-dir /tmp/agentflow-target
```

- [ ] Formatting passes.
- [ ] Clippy has no warnings.
- [ ] Workspace compile check passes.
- [ ] No accidental debug prints, temporary fixtures, or local paths are present.

## 3. Core Test Matrix

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

## 4. Integration Smoke Tests

```bash
cargo run -p agentflow-agents --example react_agent --target-dir /tmp/agentflow-target
cargo run -p agentflow-agents --example multi_agent --target-dir /tmp/agentflow-target
cargo run -p agentflow-agents --example hybrid_workflow_agent --target-dir /tmp/agentflow-target
```

- [ ] ReAct agent example runs.
- [ ] Multi-agent example runs.
- [ ] DAG + Agent hybrid example runs.
- [ ] Skill examples validate and list tools through the CLI.
- [ ] A local MCP skill can validate, list tools, and run a tool call.

## 5. Runtime Contracts

- [ ] `AgentRunResult` golden fixture changes are intentional and reviewed.
- [ ] Workflow checkpoint resume still skips completed nodes.
- [ ] `AgentNode` output includes `response`, `session_id`, `stop_reason`, and
      `agent_result`.
- [ ] `WorkflowTool` exposes schema and timeout behavior as documented.
- [ ] Trace output links workflow, agent, tool, and MCP calls where applicable.

## 6. Documentation

- [ ] `README.md` reflects the current release positioning.
- [ ] `docs/AGENT_RUNTIME.md` matches runtime behavior.
- [ ] `docs/SKILL_FORMAT.md` matches `SKILL.md` / `skill.toml` behavior.
- [ ] MCP skill docs cover server naming, tool naming, and error behavior.
- [ ] Release notes include breaking changes, migration notes, and known issues.
- [ ] Examples referenced by docs compile or run.

## 7. Version And Packaging

- [ ] Crate versions are updated consistently.
- [ ] `Cargo.lock` is intentionally updated.
- [ ] Feature flags and default features are reviewed.
- [ ] `cargo package --list` for publishable crates excludes local artifacts.
- [ ] Release notes and tag name match the version.

## 8. Final Gate

```bash
git diff --check
git status --short --ignored
```

- [ ] No whitespace errors.
- [ ] No unintended tracked changes.
- [ ] Release commit is tagged only after all required checks pass.
- [ ] Any skipped check is recorded in the release notes with owner and reason.
