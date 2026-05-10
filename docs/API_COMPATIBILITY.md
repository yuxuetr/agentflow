# API Compatibility Policy

This document defines how AgentFlow evolves public Rust APIs, manifests, wire
schemas, and persisted data after the v1 stability inventory in
`docs/STABILITY.md`.

## Versioning Rules

AgentFlow uses semantic versioning for published crates and binaries:

- Patch: bug fixes, documentation, performance improvements, and compatible
  validation tightening for invalid inputs.
- Minor: additive APIs, new optional manifest fields, new node/tool/event
  kinds, new CLI flags, and new server endpoints.
- Major: removal or rename of stable fields, changed required method
  signatures, changed persisted schema semantics, or incompatible wire formats.

Pre-v1 releases may still make breaking changes, but every breaking change to a
Stable or Beta surface must include a migration note in release documentation.

## Rust API Compatibility

Stable traits may add:

- Default methods.
- New helper types.
- New enum variants only when the enum is explicitly documented as open.
- New optional struct fields only when serde defaults preserve old inputs.

Stable traits must not change:

- Existing required method names, parameters, or return types.
- Existing semantics of `Result` success vs failure.
- Existing serialized field names for public structs and enums.

Closed enums, including `AgentStepKind` and `AgentEvent`, may receive new
variants in AgentFlow releases. Consumers must handle unknown variants at wire
boundaries by ignoring or preserving them when possible; Rust pattern matches in
downstream code should include a wildcard arm when compiling against future
versions matters.

## Manifest Compatibility

Manifest readers should be liberal and writers should be conservative:

- Unknown fields are accepted when the loader already supports serde defaults or
  ignores extension maps.
- New optional fields must have defaults.
- New required fields require either a schema version bump or an explicit
  compatibility rule.
- Existing field names and meanings must not be reused for different behavior.
- Package installers must validate checksums before trusting downloaded
  marketplace artifacts.

Current manifest owners:

| Manifest | Owner | Version key |
| --- | --- | --- |
| `SKILL.md` | `agentflow-skills` | none; compatibility by field set |
| `skill.toml` | `agentflow-skills::SkillManifest` | `[skill].version` identifies the skill package, not the schema |
| `plugin.toml` | `agentflow-core::plugin::PluginManifest` | `plugin.protocol`, currently `agentflow.plugin/1` |
| Remote marketplace TOML | `agentflow-skills::RemoteMarketplaceManifest` | `schema_version` |

## Server API Compatibility

Server endpoints use JSON request/response bodies and stable field names once
documented. Compatible changes include:

- Adding optional request fields.
- Adding response fields.
- Adding endpoints.
- Adding SSE event `kind` values.
- Adding values to status-like strings when old values keep their meaning.

Breaking changes include:

- Removing or renaming response fields.
- Changing route paths.
- Changing a field from optional to required.
- Changing run/event ordering guarantees.
- Reusing an existing event `kind` for a different payload shape.

Clients should tolerate unknown JSON fields and unknown SSE event kinds.

## Persistence and Migration

Persisted schemas must support rolling upgrades where possible:

- New columns should be nullable or have defaults.
- Data migrations must be idempotent.
- Readers should accept the previous stable shape for at least one minor
  release after a writer changes format.
- Checkpoint readers must continue accepting legacy raw JSON values as
  `FlowValue::Json`.

Trace and checkpoint schema changes must update:

- `docs/TRACE_PERSISTENCE_SCHEMA.md` or `docs/CHECKPOINT_RECOVERY.md`.
- Tests that round-trip the old and new shapes.
- Release notes with a restore/rollback note when applicable.

## Deprecation Process

Deprecating a stable API requires:

1. Marking the API deprecated in Rust docs or user docs.
2. Providing the replacement path.
3. Keeping the old path working for at least one minor release.
4. Emitting a warning when there is a runtime or CLI path to do so.
5. Removing only in a major release unless the surface is explicitly
   Experimental.

## Compatibility Checklist

Before merging a change to a Stable or Beta surface:

- Identify the surface in `docs/STABILITY.md`.
- Decide whether the change is additive, behavioral, or breaking.
- Add round-trip tests for serialized schemas.
- Update the owning user document.
- Add migration notes for breaking or behavior-changing updates.
- Prefer feature flags or new fields over changing existing behavior in place.
