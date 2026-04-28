# Skill Registry

Skill registry indexes let a team share local Skill packages without requiring
remote downloads or a marketplace. The current implementation is intentionally
local-first: an index resolves a skill directory that already exists beside the
index file or at an absolute path.

## Local Index File

Use `skills.index.toml` as the conventional file name:

```toml
schema_version = 1
name = "team-skills"
description = "Shared skills for this repository."

[[skills]]
name = "mcp-basic"
version = "1.0.0"
path = "skills/mcp-basic"
manifest = "SKILL.md"
manifest_sha256 = "sha256:<optional manifest digest>"
aliases = ["mcp-demo"]
channel = "stable"
```

Fields:

- `schema_version`: required compatibility marker. Version `1` is the only
  supported schema. Future incompatible changes must increment this value.
- `name`: human-readable index name shown by CLI commands.
- `description`: optional index description.
- `skills`: non-empty list of skill entries.
- `skills[].name`: canonical skill name. It must match the loaded manifest
  `skill.name`.
- `skills[].version`: semantic version. It must match the loaded manifest
  version.
- `skills[].path`: skill directory. Relative paths are resolved from the index
  file directory.
- `skills[].manifest`: optional manifest path relative to the skill directory.
  When omitted, AgentFlow detects `skill.toml` first, then `SKILL.md`.
- `skills[].manifest_sha256`: optional SHA-256 lock for the manifest file. The
  value can be raw hex or prefixed with `sha256:`.
- `skills[].aliases`: optional alternate names accepted by `resolve` and
  `install`. Aliases share the same uniqueness namespace as canonical names.
- `skills[].channel`: optional distribution channel such as `stable`, `beta`, or
  an internal rollout ring.

## Compatibility Policy

Schema version `1` is local-path based and has no network source fields. Tools
must reject unsupported `schema_version` values rather than guessing. Optional
fields can be added to schema version `1` only when older clients can safely
ignore them.

The canonical identity is `name + version`. Aliases are lookup conveniences, not
separate packages. A release process should update the manifest version and the
index entry version together.

## Manifest Locking

`manifest_sha256` is a lightweight integrity check for the manifest selected by
`manifest` or manifest auto-detection. Use it when:

- an organization wants reviewable changes to Skill instructions, tools, MCP
  servers, or permissions;
- a shared index points at a directory that may change over time;
- a release branch needs to pin a known manifest while allowing non-manifest
  files such as examples or references to evolve separately.

The current lock covers only the manifest file, not the full directory tree. It
is not a supply-chain signature and does not replace code review for scripts,
MCP servers, or referenced knowledge files.

## Local Workflow

Validate and inspect an index:

```bash
cargo run -p agentflow-cli -- skill index validate agentflow-skills/examples/skills.index.toml
cargo run -p agentflow-cli -- skill index list agentflow-skills/examples/skills.index.toml
cargo run -p agentflow-cli -- skill index resolve agentflow-skills/examples/skills.index.toml mcp-demo
```

Install a resolved skill into a local skills directory:

```bash
cargo run -p agentflow-cli -- skill install agentflow-skills/examples/skills.index.toml mcp-demo \
  --dir /tmp/agentflow-skills
cargo run -p agentflow-cli -- skill validate /tmp/agentflow-skills/mcp-basic
cargo run -p agentflow-cli -- skill list-tools /tmp/agentflow-skills/mcp-basic
```

`skill install` copies the resolved local directory to `<target>/<skill-name>`.
It refuses to overwrite an existing directory unless `--force` is passed.

## Upgrade Model

The local upgrade path is explicit:

1. Update the Skill manifest and files.
2. Bump the manifest version.
3. Update the index entry version.
4. Recompute `manifest_sha256` when the index uses a lock.
5. Run `skill index validate`.
6. Reinstall with `skill install ... --force` when the destination should be
   replaced.

There is no automatic background upgrade in the current model.

## Remote Registry Boundary

Future remote registry or Git install support should extend this local model
without changing local semantics:

- keep `resolve` returning a concrete skill directory and manifest path before
  install;
- download or clone into a cache first, then reuse the same validation and copy
  path used by local install;
- add source fields such as `git`, `rev`, `subdir`, or `archive_url` only under
  a compatible schema plan;
- require explicit trust and overwrite decisions instead of silently replacing
  installed skills;
- keep remote network access out of `skill index validate` unless the user asks
  for a remote validation mode.

This preserves the current no-network CI path while leaving room for Git-backed
or marketplace-backed distribution.
