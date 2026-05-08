# Marketplace

AgentFlow is moving from local-only Skill catalogs toward a unified remote
marketplace for both Skills and Plugins. The remote marketplace schema is the
shared package index that future `agentflow marketplace ...` commands will
fetch, cache, verify, and install from.

## Schema

Remote marketplace manifests use TOML and schema version `1`.

```toml
schema_version = 1
name = "agentflow-community"
description = "Remote catalog for AgentFlow Skills and Plugins"
homepage = "https://registry.example.com"

[[entries]]
name = "rust-expert"
version = "1.0.0"
type = "skill"
aliases = ["rust"]
description = "Rust code review assistant"

[entries.source]
registry_url = "https://registry.example.com/marketplace.toml"
artifact_url = "https://registry.example.com/skills/rust-expert-1.0.0.tar.gz"
checksum_sha256 = "sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

[entries.signature]
algorithm = "minisign"
key_id = "agentflow-community"
value = "base64-or-armored-signature"

[[entries]]
name = "echo-plugin"
version = "0.1.0"
type = "plugin"

[entries.source]
registry_url = "https://registry.example.com/marketplace.toml"
artifact_url = "https://registry.example.com/plugins/echo-plugin-0.1.0.tar.gz"
checksum_sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
```

## Fields

- `schema_version`: currently `1`.
- `name`: registry display name.
- `description`: optional human-readable description.
- `homepage`: optional HTTP(S) homepage.
- `entries[]`: package entries. The manifest must contain at least one entry.
- `entries[].name`: package name.
- `entries[].version`: semver version.
- `entries[].type`: `skill` or `plugin`.
- `entries[].aliases`: optional lookup aliases, unique per package type.
- `entries[].source.registry_url`: canonical HTTP(S) URL of the registry
  document this entry came from.
- `entries[].source.artifact_url`: HTTP(S) URL of the package archive or
  repository snapshot to install.
- `entries[].source.checksum_sha256`: SHA-256 digest of the artifact, either
  raw 64-char hex or `sha256:<hex>`.
- `entries[].signature`: optional supply-chain signature metadata. Signature
  verification is a follow-up task; the schema reserves `algorithm`, `key_id`,
  and `value` now so catalogs can start publishing it.

## Validation

The schema is implemented in `agentflow-skills::remote_marketplace`.

Current validation enforces:

- supported schema version;
- non-empty registry and entry names;
- at least one entry;
- semver package versions;
- unique package names and aliases per package type;
- HTTP(S) registry and artifact URLs;
- well-formed SHA-256 artifact checksums;
- non-empty signature fields when a signature block is present.

Skills and Plugins may share the same package name because their install
targets and runtime surfaces are distinct. Within a package type, names and
aliases are unique lookup keys.

## Roadmap

The following pieces are intentionally separate follow-up tasks:

- read-only remote registry HTTP client;
- local cache layout and offline lookup;
- checksum and signature verification during install;
- `agentflow marketplace search/install/update/verify` CLI;
- migration path from local `agentflow skill marketplace` files to the unified
  remote marketplace command group.
