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

`RemoteMarketplaceCache::new()`'s library-level default verifier is still
`ChecksumSha256SignatureVerifier` — it accepts `signature.algorithm =
"checksum-sha256"` or `"sha256"` and compares `signature.value` to the
artifact SHA-256. This is a bootstrap verifier for deterministic tests and
purely local registries; it is **not** a real signature — an attacker who
controls the artifact can trivially recompute the checksum it compares
against. **The CLI does not use this default for non-local registries**; see
[CLI Default Verifier Selection](#cli-default-verifier-selection) below.

A real `Ed25519SignatureVerifier` also ships in `agentflow-skills` and is what
the CLI wires up by default for HTTP(S) registries. It loads a publisher's
Ed25519 public key from a keys directory (one `<key_id>.pub` file per
publisher, base64-encoded raw 32-byte key material) and verifies a
base64-encoded detached signature over the raw artifact bytes.

Artifacts without a signature are still allowed at the cache layer when the
active verifier doesn't require one — signature *requirements* are a CLI/
policy decision layered on top. The cache records whether a signature was
checked in `CachedMarketplaceArtifact::signature_checked`, and — since T0.1 —
*what kind* of check actually ran in `CachedMarketplaceArtifact::
signature_verification` (`unsigned` / `checksum_only` /
`cryptographic_signature`). Don't infer verification strength from
`signature_checked` alone: it is `true` for both a checksum-only match and a
real Ed25519 signature, and only `signature_verification` tells them apart.

## CLI

The top-level marketplace CLI works with either an HTTP(S) registry URL or a
local remote marketplace TOML file:

```bash
agentflow marketplace search https://registry.example.com/marketplace.toml rust --type skill
agentflow marketplace update https://registry.example.com/marketplace.toml
agentflow marketplace install https://registry.example.com/marketplace.toml rust-expert --type skill --dir ~/.agentflow/skills
agentflow marketplace verify https://registry.example.com/marketplace.toml rust-expert --type skill
agentflow marketplace verify https://registry.example.com/marketplace.toml rust-expert --type skill --strict
```

Command behavior:

- `search`: list matching entries from the remote marketplace catalog.
- `update`: fetch or load the registry manifest and write it under
  `<cache>/registries/<marketplace>.toml`.
- `install`: resolve a package, download or reuse its artifact, verify checksum
  and signature policy, write the verified artifact into the local cache, and
  unpack/install it into the package-specific runtime directory.
- `verify`: verify one cached package, or all matching cached packages, without
  contacting the artifact URL.

Install options:

- `--dir <path>` overrides the install root. Defaults to `~/.agentflow/skills`
  for Skills and `~/.agentflow/plugins` for Plugins.
- `--force` overwrites an existing installed package directory.
- `--cache-only` stops after verified cache write/verification and does not
  unpack into the runtime install directory.
- `verify --strict` also requires signature metadata to be present and
  successfully checked. Without `--strict`, unsigned artifacts may be verified
  by checksum only.
- `--allow-unsigned` and `--keys-dir` — see
  [CLI Default Verifier Selection](#cli-default-verifier-selection).

## CLI Default Verifier Selection

**T0.1** (evaluation §5 finding 1): `install` and `verify` pick their
`MarketplaceSignatureVerifier` based on whether `registry` is a genuinely
remote registry:

| `registry` argument | Default verifier | Notes |
| --- | --- | --- |
| `http://` / `https://` URL | `Ed25519SignatureVerifier { require_signature: true }` | Every entry **must** carry a valid `[signature]` block with `algorithm = "ed25519"`, or verification fails. |
| local file path | `ChecksumSha256SignatureVerifier` | Unchanged bootstrap behavior — local manifests have no network-facing publisher identity to verify against. |

Keys are read from `~/.agentflow/marketplace-keys/<key_id>.pub` by default;
override the directory with `--keys-dir <path>`. Each file holds a single
base64-encoded 32-byte raw Ed25519 public key (see
`Ed25519SignatureVerifier` rustdoc in `agentflow-skills/src/
remote_marketplace.rs` for the `openssl` command that produces one).

For a non-local registry, this means by default:

- an entry with **no** `[signature]` block is rejected
  (`"... has no [signature] block but Ed25519 verifier requires one"`);
- an entry signed with anything other than `algorithm = "ed25519"` (including
  the old `checksum-sha256` bootstrap style) is rejected
  (`"Ed25519 verifier rejected algorithm '...'"`);
- an entry with a tampered artifact or a signature that doesn't verify against
  the named `key_id`'s public key is rejected.

`--allow-unsigned` is the explicit opt-out: it downgrades a non-local registry
back to `ChecksumSha256SignatureVerifier` and prints a loud warning to stderr
before proceeding. **Do not use it for production installs** — it does not
prove the artifact came from a trusted publisher, only that the manifest and
artifact bytes agree with each other.

```bash
# Default: rejected unless the entry carries a valid ed25519 signature.
agentflow marketplace install https://registry.example.com/marketplace.toml rust-expert --type skill

# Explicit, loudly-warned downgrade to checksum-only verification.
agentflow marketplace install https://registry.example.com/marketplace.toml rust-expert --type skill --allow-unsigned

# Point at a non-default publisher keys directory.
agentflow marketplace verify https://registry.example.com/marketplace.toml rust-expert --type skill --keys-dir ./ci-marketplace-keys
```

Package artifacts are `.tar` or `.tar.gz` archives. The archive may contain the
manifest at the root or inside a single top-level directory:

- Skill packages must contain `SKILL.md` and pass `SkillLoader` validation.
- Plugin packages must contain `plugin.toml` and pass plugin manifest
  validation. Plugin install requires an `agentflow` binary built with the
  `plugin` feature.

Archive extraction rejects absolute paths, `..` traversal, symlinks, hardlinks,
duplicate paths, oversized files, and other non-file/non-directory entries
before copying into the install root. Plugin package entrypoints must resolve
to a file inside the package root; absolute entrypoints and `..` traversal are
rejected before install.

## Signing Policy Boundary

Every artifact must match `entries[].source.checksum_sha256`; checksum
mismatches are always fatal. Signature enforcement has two layers:

- the cache calls the configured `MarketplaceSignatureVerifier` whenever an
  `entries[].signature` block is present (or unconditionally, if the verifier
  requires one — see `Ed25519SignatureVerifier`'s `require_signature`);
- CLI policy (registry_kind → verifier selection, `--strict`) decides which
  verifier is active and whether a missing signature is acceptable.

Two verifiers ship today:

- `ChecksumSha256SignatureVerifier` — bootstrap verifier for deterministic
  tests and simple local registries. It treats the signature value as
  another SHA-256 checksum and proves only that the artifact matches the
  catalog metadata; it provides no publisher identity, transparency, expiry,
  revocation, or key rotation. This is still `RemoteMarketplaceCache::new()`'s
  library-level default, and what the CLI uses for local manifest files or
  when `--allow-unsigned` is passed for a remote registry.
- `Ed25519SignatureVerifier` — real cryptographic signature verification
  against a publisher's Ed25519 public key. **Since T0.1, this is the CLI's
  default for any HTTP(S) registry** (see [CLI Default Verifier
  Selection](#cli-default-verifier-selection)); it still has no transparency
  log, expiry, revocation, or key rotation of its own, but it does prove the
  artifact was signed by whoever holds the private key for the named
  `key_id` — the checksum-only verifier proves nothing beyond internal
  consistency.

Registries that need transparency/revocation/rotation on top of raw Ed25519
signature checking should implement `MarketplaceSignatureVerifier` against
sigstore, minisign with a key-rotation policy, or another signing system, and
pass it to `RemoteMarketplaceCache::with_client_and_verifier`. `agentflow
marketplace verify --strict` remains an orthogonal, additional gate — it
requires signature metadata to be present and successfully checked
regardless of which verifier is configured.

## Local signing

For local development and the in-tree marketplace tests, a Skill or
Plugin package is signed by hashing the archive bytes and pasting the
hex digest into the entry's `signature.value`. This is what the
default `ChecksumSha256SignatureVerifier` checks. The flow is:

1. Build a deterministic `.tar.gz` of the package directory. Determinism
   matters because the signature is derived from the bytes — any change
   in mtime, file order, or compression settings invalidates the
   signature. The reference build used by the fixture tests is in
   `agentflow-skills/tests/marketplace_signed.rs::build_signed_archive`
   (fixed mtime, fixed uid/gid, fixed mode, sorted entries).
2. Compute `sha256_hex(archive_bytes)`.
3. Set both `source.checksum_sha256` and `signature.value` to the
   resulting digest. Set `signature.algorithm = "checksum-sha256"` and
   pick a stable `signature.key_id` (e.g. `"agentflow-dev-test"`).
4. Publish the archive at `source.artifact_url` (or, for offline
   tests, hand the bytes to `RemoteMarketplaceCache::cache_artifact_bytes`).

This checksum-style signing only satisfies `ChecksumSha256SignatureVerifier` —
against the CLI's default `Ed25519SignatureVerifier` for a remote registry, an
entry signed this way is rejected outright (`algorithm` must be `"ed25519"`).
For a real Ed25519-signed fixture, generate a keypair and sign the archive
bytes directly; see `Ed25519SignatureVerifier` rustdoc in
`agentflow-skills/src/remote_marketplace.rs` for the `openssl` commands and
`agentflow-cli/tests/marketplace_cli_tests.rs`'s
`marketplace_verify_remote_registry_accepts_valid_ed25519_signature_by_default`
for a full worked example (keypair → `.pub` file → signed entry → CLI
`verify`).

The cache layer accepts archives with or without a `signature` block when the
active verifier doesn't require one. The strict policy (`verify --strict` on
the CLI, or the `marketplace.require_signature_verification`
security-profile flag) is layered on top: when set, callers must reject any
cached artifact whose `CachedMarketplaceArtifact::signature_checked` is
`false`.

Tests covering both paths live alongside the fixture archives:

```text
agentflow-skills/tests/fixtures/signed/skill-rust-expert/SKILL.md
agentflow-core/tests/fixtures/signed/plugin-echo/plugin.toml
agentflow-skills/tests/marketplace_signed.rs   # strict + non-strict (checksum-only verifier)
agentflow-core/tests/plugin_signed_fixture.rs  # manifest sanity
agentflow-cli/tests/marketplace_cli_tests.rs   # T0.1: remote-registry default Ed25519 verification + --allow-unsigned
```

## Offline Flow

After an artifact has been cached, `verify` or `install --cache-only` can run
with a local copy of the marketplace TOML:

```bash
agentflow marketplace update https://registry.example.com/marketplace.toml
agentflow marketplace verify ~/.agentflow/marketplace/cache/registries/agentflow-community.toml rust-expert --type skill
```

This checks the cached bytes against the catalog checksum and signature metadata
without downloading the artifact again.

## Current Boundaries

The implemented remote marketplace layer covers catalog schema, read-only
registry fetch, verified artifact caching, offline cache verification, safe
archive unpack, package-specific install into Skill or Plugin roots, and (T0.1)
CLI-default real Ed25519 signature verification for non-local registries with
an explicit, warned `--allow-unsigned` opt-out.

It does not yet implement background update jobs, dependency resolution
between packages, or transparency-log/expiry/revocation/key-rotation on top of
raw Ed25519 signature checking — registries needing those should implement
`MarketplaceSignatureVerifier` against sigstore or a similar system.
