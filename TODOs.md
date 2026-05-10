# AgentFlow TODOs

Last updated: 2026-05-10

This file is the short-term execution queue only. Current implemented status is
tracked in `docs/CURRENT_STATUS.md`; future direction is tracked in
`RoadMap.md`; historical evaluation context is in
`PROJECT_EVALUATION_2026-05-01.md`.

## Active Queue

No active P0-P4 tasks remain after the 2026-05-10 documentation convergence
pass.

## Recently Closed

- P3.3 Web UI Run Console.
- P4.1 v1 stable interface inventory.
- P4.2 official ecosystem samples.
- P4.3 documentation convergence.

## Next Candidate Work

Pick one item at a time, expand it into concrete subtasks, then commit code and
sync this file after each completed feature.

- V1 contract tests:
  - Add round-trip tests for stable manifest and event schemas.
  - Add compatibility fixtures for `FlowValue` checkpoint formats.
- Platform reliability:
  - Add run retention and cleanup policy to `agentflow-server` / `agentflow-db`.
  - Add end-to-end server run tests covering submit, cancel, graph, history,
    and SSE reconnect.
- Distributed execution:
  - Add worker auth/admission checks.
  - Add resource limit tests for worker-executed DAG nodes.
- Ecosystem packaging:
  - Package `examples/ecosystem/` samples as installable archives.
  - Add signed marketplace fixture artifacts for local verification tests.
- Web UI productization:
  - Add provider configuration diagnostics endpoint and UI panel.
  - Add event filtering/search for agent, tool, MCP, RAG, and node events.

## Quality Gates

For each task:

- Read relevant code/docs first.
- Implement the smallest coherent feature.
- Run focused tests or validation commands.
- Commit the feature with a conventional message.
- Update this TODO file only after the feature commit succeeds.
