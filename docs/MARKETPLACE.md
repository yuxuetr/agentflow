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

## Read-Only HTTP Registry

Remote registries are plain HTTP(S) endpoints that serve the TOML manifest.
The first client implementation is `RemoteMarketplaceClient`:

```rust
let client = agentflow_skills::RemoteMarketplaceClient::new();
let manifest = client
  .fetch_manifest("https://registry.example.com/marketplace.toml")
  .await?;
```

The client is deliberately read-only. It validates that the registry URL is
HTTP(S), sends a GET request, rejects non-2xx responses, parses TOML, and runs
the same schema validation as local `RemoteMarketplaceManifest::load`.

It does not write cache state, install packages, or verify signatures yet.

## Local Cache And Verification

`RemoteMarketplaceCache` stores verified artifacts under:

```text
~/.agentflow/marketplace/cache/artifacts/<type>/<name>/<version>/<sha256>.pkg
```

Package names and versions are path-sanitized before they are used as
directories. The cache API verifies the artifact before writing it:

1. validate the marketplace entry;
2. compute the artifact SHA-256 and compare it with
   `entries[].source.checksum_sha256`;
3. run the configured `MarketplaceSignatureVerifier`;
4. write the artifact atomically via a temporary file and rename.

The default verifier is `ChecksumSha256SignatureVerifier`. It accepts
`signature.algorithm = "checksum-sha256"` or `"sha256"` and compares
`signature.value` to the artifact SHA-256. This is useful for deterministic
tests and bootstrap registries; production registries should plug in a verifier
for a real signing system such as minisign or sigstore.

Artifacts without a signature are still allowed at this layer because signature
requirements are a CLI/policy decision. The cache records whether a signature
was checked in `CachedMarketplaceArtifact::signature_checked`.

## Roadmap

The following pieces are intentionally separate follow-up tasks:

- package-specific unpack/install integration from a verified cached artifact
  into `~/.agentflow/skills` or `~/.agentflow/plugins`;
- migration path from local `agentflow skill marketplace` files to the unified
  remote marketplace command group.

## CLI

The top-level marketplace CLI works with either an HTTP(S) registry URL or a
local remote marketplace TOML file:

```bash
agentflow marketplace search https://registry.example.com/marketplace.toml rust --type skill
agentflow marketplace update https://registry.example.com/marketplace.toml
agentflow marketplace install https://registry.example.com/marketplace.toml rust-expert --type skill
agentflow marketplace verify https://registry.example.com/marketplace.toml rust-expert --type skill
```

`install` currently downloads and verifies the artifact into the marketplace
cache. It does not yet unpack a Skill into `~/.agentflow/skills` or a Plugin
into `~/.agentflow/plugins`; that final package-specific handoff is the next
installation integration step.
