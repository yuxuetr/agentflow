# Marketplace

AgentFlow is moving from local-only Skill catalogs toward a unified remote
marketplace for both Skills and Plugins. The remote marketplace schema is the
shared package index that `agentflow marketplace ...` commands can fetch,
cache, verify, and install from.

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
- `entries[].signature`: optional supply-chain signature metadata. The cache
  layer verifies this block through `MarketplaceSignatureVerifier` before it
  writes a downloaded artifact.

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

The registry client is deliberately read-only. It validates that the registry
URL is HTTP(S), sends a GET request, rejects non-2xx responses, parses TOML,
and runs the same schema validation as local `RemoteMarketplaceManifest::load`.
Artifact download and verification happen in `RemoteMarketplaceCache`.

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

## CLI

The top-level marketplace CLI works with either an HTTP(S) registry URL or a
local remote marketplace TOML file:

```bash
agentflow marketplace search https://registry.example.com/marketplace.toml rust --type skill
agentflow marketplace update https://registry.example.com/marketplace.toml
agentflow marketplace install https://registry.example.com/marketplace.toml rust-expert --type skill
agentflow marketplace verify https://registry.example.com/marketplace.toml rust-expert --type skill
```

Command behavior:

- `search`: list matching entries from the remote marketplace catalog.
- `update`: fetch or load the registry manifest and write it under
  `<cache>/registries/<marketplace>.toml`.
- `install`: resolve a package, download its artifact, verify checksum and
  signature policy, then write the verified artifact into the local cache.
- `verify`: verify one cached package, or all matching cached packages, without
  contacting the artifact URL.

`install` currently stops at the verified marketplace artifact cache. It does
not yet unpack a Skill into `~/.agentflow/skills` or a Plugin into
`~/.agentflow/plugins`; that package-specific handoff remains the next install
integration step.

## Offline Flow

After an artifact has been cached, `verify` can run with a local copy of the
marketplace TOML:

```bash
agentflow marketplace update https://registry.example.com/marketplace.toml
agentflow marketplace verify ~/.agentflow/marketplace/cache/registries/agentflow-community.toml rust-expert --type skill
```

This checks the cached bytes against the catalog checksum and signature metadata
without downloading the artifact again.

## Current Boundaries

The implemented remote marketplace layer covers catalog schema, read-only
registry fetch, verified artifact caching, offline cache verification, and the
top-level CLI entry points. It intentionally does not yet define a package
archive layout or unpack cached artifacts into the runtime install locations.

The package-specific handoff will reuse the existing local installers:

- Skills: validated package contents should flow into `agentflow skill install`
  semantics and land under `~/.agentflow/skills`.
- Plugins: validated package contents should flow into `agentflow plugin install`
  semantics and land under `~/.agentflow/plugins`.
