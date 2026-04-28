---
name: Release
about: Track the manual checks that remain after the CI release gate passes.
title: "Release vX.Y.Z"
labels: release
assignees: ""
---

## Scope

- [ ] Confirm release target, version, branch, and tag name.
- [ ] Review `RoadMap.md`, `TODOs.md`, and release notes for completed work.
- [ ] Confirm ignored local files and generated outputs are not required in the release commit.

## CI Gate

- [ ] `release gate` passed on the release branch or tag.
- [ ] Any failed or skipped CI check has an owner, reason, and release-note entry.

## Manual Contracts

- [ ] `AgentRunResult` golden fixture changes are intentional and reviewed.
- [ ] Workflow checkpoint resume behavior is still compatible with documented recovery.
- [ ] Agent, workflow, tool, and MCP trace output still links a mixed run.
- [ ] Feature flags and default features are reviewed.

## Documentation And Packaging

- [ ] Release notes include breaking changes, migration notes, and known issues.
- [ ] User-facing docs match the release behavior.
- [ ] Crate versions and `Cargo.lock` changes are intentional.
- [ ] `cargo package --list` for publishable crates excludes local artifacts.
