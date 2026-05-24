# Audit: agentflow-skills

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-skills/
**Crate version**: 0.1.0 (per `agentflow-skills/Cargo.toml`)
**Layer**: L3 (Agent / Orchestration)
**Stability tier**: alpha (workspace `description` markets it as "declarative skill manifests…", but `0.1.0` and the still-evolving marketplace/signature story put this short of beta; `CLAUDE.md` does not assign an explicit tier)

## Scope summary

`agentflow-skills` turns declarative `SKILL.md` (YAML frontmatter + Markdown body) or `skill.toml` manifests into a runnable `ReActAgent` via `SkillBuilder`. Twelve source files, ~6 KLOC, covering:

- Manifest schema (`manifest.rs`, `skill_md.rs`) — two surface formats sharing one in-memory `SkillManifest`.
- Manifest loader/validator (`loader.rs`) — file-system probe (`skill.toml` wins over `SKILL.md`), tool/MCP/knowledge/memory hard- and soft-validation.
- Agent builder (`builder.rs`) — persona stitching, `ToolRegistry` + `SandboxPolicy` assembly, per-tool `os_sandbox` resolution, three `MemoryStore` flavors (`SessionMemory` / `SqliteMemory` / `SemanticMemory`).
- MCP adapter (`mcp_tools.rs`) — `McpClientPool` (lazy reconnect, `Semaphore` concurrency guard, per-call timeout) and `McpToolAdapter` exposing MCP tools through the `agentflow_tools::Tool` trait.
- Local registry (`index.rs`) — `skills.index.toml` schema with semver, optional `manifest_sha256`, aliases, channels.
- Local + remote marketplace (`marketplace.rs`, `remote_marketplace.rs`) — catalog of indexes (local/organization/remote), read-only HTTP fetch client, `RemoteMarketplaceCache` with checksum + pluggable signature verifier.
- Tool-admission policy resolver (`policy.rs`) — 6-layer precedence (CLI deny > CLI allow > skill deny > skill allow > MCP server capability > `ToolPolicy` default).
- Skill-declared answer validators (`validator.rs`) — `none` / `regex` / `command` kinds for the eval harness.

Manifest dependency surface stays inside L1/L2 plus the L3 peer `agentflow-agents`; no L4 crate is touched. CLI surface lives in `agentflow-cli` (audited elsewhere).

## Findings

### CRITICAL

_None._ The crate has correct precedence layering, gating around all unsafe paths, and the only non-test `expect` is mathematically infallible (`reqwest::Client::builder().no_proxy().build()`).

### MAJOR

- [M1] No path-traversal guard on `[[knowledge]]`, `[[mcp_servers]]` command/args, and shell `allowed_paths` — `agentflow-skills/src/builder.rs:144-148`, `src/builder.rs:458-464`, `src/builder.rs:478-487`, `src/loader.rs:236-254`.
  **What**: `resolve_knowledge_path` (`loader.rs:237`) joins `skill_dir` with the user-controlled `path`, then feeds it directly through `glob::glob`/`PathBuf::exists`. The same pattern in `build_persona` blindly `std::fs::read_to_string`s the result, so a manifest containing `path = "../../../etc/passwd"` or `path = "/etc/passwd"` (the code explicitly handles `Path::is_absolute`) is read and stuffed into the agent's system prompt. `resolve_skill_relative_command_part` (`builder.rs:458`) only joins `./` or `../` prefixes — but `mcp_command_allowlist` lets a manifest's `[[mcp_servers]] command = "../bin/evil"` resolve to an arbitrary host path before the executable-name allowlist check happens (the allowlist is matched on `Path::file_name`, so the directory traversal goes unchecked).
  **Why it matters**: A signed-but-malicious skill (or one whose signature verifier is the bootstrap `ChecksumSha256SignatureVerifier`, which only checks that `signature.value == sha256(bytes)` — i.e. self-attests) can exfiltrate host secrets into the persona, or — worst case — pin an MCP command to an arbitrary path on disk and bypass the allowlist filtering. The marketplace doc already calls out "Unbounded remote code execution from marketplace packages" as a known risk; this raises the floor on it.
  **Fix**: (a) Add `Path::components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)))` guards in `resolve_knowledge_path` and `resolve_skill_relative_command_part`; (b) keep absolute paths only when an explicit `[security] allow_absolute_paths` opt-in is set; (c) bound `allowed_paths` resolution similarly. Each rejection should surface as `SkillError::ValidationError` with the offending segment, not a tracing warning.

- [M2] Bootstrap signature verifier is a self-checksum, not a signature — `agentflow-skills/src/remote_marketplace.rs:267-292`.
  **What**: `ChecksumSha256SignatureVerifier` accepts any entry whose `signature.value` equals `sha256(artifact_bytes)`. There is no key material, no asymmetric verification — only a duplicate of `MarketplaceSource::checksum_sha256` (which is *already* enforced separately at `cache_artifact_bytes:218-226`). The doc comment correctly says "Production registries should install a verifier for their chosen signature system," but the only place a real verifier could be plugged in is `RemoteMarketplaceCache::with_client_and_verifier`, and the *default constructor* (`RemoteMarketplaceCache::new`, line 122) hard-codes the bootstrap verifier. Calls from the CLI install path almost certainly hit `new`, so the production codebase currently ships with no actual signature verification.
  **Why it matters**: Any operator using the default `RemoteMarketplaceCache` while believing `--require-signature` provides cryptographic assurance is mistaken — the gate only proves the publisher knew the artifact's SHA-256, which any tampering MitM also knows. RoadMap.md line 103 lists "Complete verified remote artifact cache to Skill/Plugin install directory" as P5 work, but the gap deserves a louder marker because the existing public API name `MarketplaceSignatureVerifier` suggests real signing.
  **Fix**: (i) rename `ChecksumSha256SignatureVerifier` to `BootstrapChecksumOnlyVerifier` and put a top-of-file doc warning that it provides *no* authenticity guarantee; (ii) make `RemoteMarketplaceCache::new` panic-document or refuse construction without an explicit verifier in release builds; (iii) ship a `MinisignVerifier` or `SigstoreVerifier` next to the bootstrap one before tagging v1.0.0.

- [M3] `RemoteMarketplaceClient::fetch_*` has neither a max-bytes cap nor ETag/conditional-fetch support — `agentflow-skills/src/remote_marketplace.rs:58-110`.
  **What**: `fetch_manifest` calls `response.text().await` and `fetch_artifact_bytes` calls `response.bytes().await` without ever consulting `content-length`, applying a `Body::limit`, or sending `If-None-Match`/`If-Modified-Since` headers. The cache layer writes whatever bytes come back. A hostile registry can return a multi-GiB blob; even a friendly registry costs full re-downloads on every refresh because nothing on the wire carries cache state.
  **Why it matters**: Both DoS (memory blow-up on a single fetch) and operational cost (unconditional re-fetch on every CLI invocation) bite. The roadmap calls for "ETag-aware client + checksum-pinned artifact cache" (RoadMap.md mentions caching under P5); the current implementation does not satisfy either half.
  **Fix**: (a) wrap the body stream and abort once it exceeds a configurable `max_artifact_bytes` (default e.g. 64 MiB for skills, larger for plugins under explicit opt-in); (b) persist `ETag` / `Last-Modified` alongside the cache file and emit conditional requests; (c) add a `reqwest::Client::builder().timeout(...)` to bound both connect and read time end-to-end.

- [M4] `os_sandbox = false` is the manifest default — `agentflow-skills/src/manifest.rs:239-241` + `src/builder.rs:242-269`.
  **What**: `SecurityConfig::default()` sets `os_sandbox: false`, and pre-P10.4.1 skills (which never had the field) parse to the same value. So the *only* skills currently getting macOS `sandbox-exec` / Linux seccomp wrapping are ones that explicitly opted in.
  **Why it matters**: Skills that author-attestation alone wouldn't justify trusting (any remote-marketplace install pre-real-signature, see M2) run `shell` / `script` interpreters under no OS-level isolation by default. The doc comment correctly explains the back-compat rationale, but for a "declarative agent capability" surface that ships an MCP discovery story and an HTTP-fetched marketplace, the default should bias toward sandboxed.
  **Fix**: For v1.0.0, flip the default to `true` for the `script` tool (which is the higher-risk case because it spawns user-supplied interpreters) and keep `shell` opt-in only if its `allowed_commands` allowlist is the SandboxPolicy default. Provide a one-shot migration message when a pre-flip skill loads.

### MINOR

- [m1] Duplicate MCP governance validation — `agentflow-skills/src/loader.rs:104-184` and `src/builder.rs:340-408`.
  **What**: `SkillLoader::validate` and `register_mcp_tools::validate_mcp_governance` enforce nearly identical allowlists (server name, command, env). Drift risk if one is updated and the other isn't.
  **Fix**: Factor a single `mcp_governance::check(manifest) -> Result<(), SkillError>` that both call. Drop the duplicate.

- [m2] `unwrap_or_else(|| Path::new("."))` for parent-dir fallback — `agentflow-skills/src/index.rs:61`, `src/marketplace.rs:74,130,158`.
  **What**: When the index/marketplace path has no parent (i.e. someone passes `"foo.toml"` with no directory), the code silently resolves relative paths against `./`. Not a security bug, but error messages thereafter reference confusingly-non-anchored paths.
  **Fix**: Convert the path to `Path::canonicalize()` (or at minimum `std::env::current_dir().join(...)`) once at entry so all downstream error reasons reference a real directory.

- [m3] `resolve_skill` is an O(n) iter-find — `agentflow-skills/src/index.rs:149-160` and `src/marketplace.rs:160-179`.
  **What**: Every skill resolution walks the full `entries` vec twice (once for the primary `name`, again for each alias). For example registries (5-20 entries) this is irrelevant; for hypothetical organization-wide registries with hundreds of skills it becomes noticeable.
  **Fix**: Add a one-time-built `BTreeMap<String, usize>` index (name + aliases → entry index) inside `SkillRegistryIndex::load` and re-use it in both `resolve_skill` and `validate_at`. Cheap and removes a future scaling cliff.

- [m4] Knowledge file content is read into memory unbounded — `agentflow-skills/src/builder.rs:152` and `:190`.
  **What**: Both `[[knowledge]]` and `references/` files are read with `std::fs::read_to_string`. A multi-MB `.md` file gets fully concatenated into the persona string, which is then shipped to the LLM on every turn.
  **Fix**: Either reject knowledge files above a configurable `max_knowledge_bytes` (default ~256 KiB) at validate-time, or auto-promote large files to a `SemanticMemory` index. Both options align with what the doc comment already promises for "Phase 3".

- [m5] `SkillError::HttpError` / `IoError` / `McpError` are stringly typed — `agentflow-skills/src/error.rs:30-37`.
  **What**: Three variants box their cause as `String`. Loses error-source chaining, breaks `Error::source()` traversal that downstream observability layers rely on.
  **Fix**: Make them `#[from] reqwest::Error`, `#[from] std::io::Error` (collides with `ReadError`; rename to `Read`/`Write`), and `#[from] agentflow_mcp::Error` respectively. Add `#[source]` wrappers if a richer context message is needed.

- [m6] `SkillMd::parse` silently drops unknown frontmatter keys — `agentflow-skills/src/skill_md.rs:42-46`.
  **What**: The struct is not `#[serde(deny_unknown_fields)]`, so typoed keys (`namee:`, `mcp_serversss:`) parse to defaults and produce a wrong-looking agent. The `manifest_compat.rs::skill_md_fixture_ignores_unknown_frontmatter_and_builds_manifest` test pins this as intentional forward-compat, but the trade-off is that pre-v1 schema evolution lets typos through silently.
  **Fix**: Add a soft `validate_frontmatter_strictness(content: &str)` pass that diffs the raw YAML key set against the known schema and emits warnings (not errors) for unknown keys. Surface them through `SkillLoader::validate`'s warnings vec.

- [m7] `RegexValidator::new` accepts unbounded patterns — `agentflow-skills/src/validator.rs:71-80`.
  **What**: A pathological pattern like `(a+)+$` compiles fine in the `regex` crate (no catastrophic backtracking thanks to RE2 engine, true) but a manifest can still set a pattern that takes hundreds of milliseconds per match. The validator runs in the synchronous assertion path, so it blocks the eval loop.
  **Fix**: Cap pattern length (e.g. 4 KiB) and reject patterns whose `Regex::estimated_size()` exceeds e.g. 10 MiB. Test in `build_validator` so eval-time matches are bounded.

- [m8] Empty `tool_permission_allowlist` is documented as "use AgentFlow's default policy" but actually means "no `ToolPolicy` is wired and no skill-level capability layer is applied" — `agentflow-skills/src/manifest.rs:210-215`, `src/builder.rs:218-226`.
  **What**: The doc on `SecurityConfig` (line 210) says "Empty allowlists mean 'use AgentFlow's default policy'; they do not mean unrestricted execution." But in `build_tool_registry` the `if !security.tool_permission_allowlist.is_empty()` branch is skipped entirely when empty, leaving the registry's `ToolPolicy` at `None` (which `agentflow_tools::ToolRegistry::execute` treats as allow-all for permissions the tool already advertises).
  **Fix**: Either install a `ToolPolicy::default()` (which the `agentflow-tools` crate maintains explicitly for this use case) when the allowlist is empty, or correct the doc to say "Empty = no per-permission gating layered on top of the per-tool sandbox; tools still execute under their own SandboxPolicy."

- [m9] `LICENSE` field on `SkillMdFrontmatter` is parsed but never propagated to `SkillManifest` — `agentflow-skills/src/skill_md.rs:49,77` vs `:179-209`.
  **What**: `into_manifest` drops `license` and `compatibility` on the floor. The CLI's `skill inspect` command (per CLAUDE.md) is meant to surface this metadata.
  **Fix**: Extend `SkillInfo` (or add a sibling `SkillMetadata { license, compatibility, … }` struct) so the marketplace UI can show license/compat info downstream.

- [m10] `examples/skills/code-reviewer/SKILL.md` is checked in but isn't covered by any automated test, and `examples/skills/rust_expert/` lives next to it under `examples/skills/` — the marketplace `examples/marketplace.toml` only references `mcp-basic`.
  **Fix**: Either add the two unused examples to `examples/skills.index.toml` and exercise them in a smoke test, or move them under `examples/legacy/` to make their non-tested status explicit.

### POSITIVE OBSERVATIONS

- **Zero non-test `unwrap()`** — the single non-test `expect` (`remote_marketplace.rs:47`) is on a `reqwest::Client::builder().no_proxy().build()` call whose only failure mode is OS-level TLS-backend init failure; the message is honest about why it's considered infallible. Matches the project's "never unwrap" rule.
- **Closed-set discriminators** — `ValidationConfig` (`manifest.rs:64`), `MarketplacePackageType` (`remote_marketplace.rs:322`), `AdmissionSource` (`policy.rs:111`) all use `#[serde(tag = "kind")]` / explicit enums so schema-version bumps are mechanically detectable.
- **`McpClientPool` shutdown discipline** — on the first failed `list_tools` (`builder.rs:303-310`), the partial set of already-spawned MCP servers is `disconnect()`-ed before returning the error. No orphan stdio subprocesses.
- **Path-prefix `~` expansion is local-only** — `expand_tilde` (`builder.rs:625`) checks the exact `~` / `~/` prefix and never spawns shells, side-stepping a common injection class.
- **Test surface is dense relative to code size** — 3 integration test files (753 LOC) plus 9 in-crate `#[cfg(test)]` modules covering parser, schema-evolution forward-compat, MCP governance, signed marketplace strict/non-strict paths, and per-tool `os_sandbox` override permutations. The `manifest_compat.rs` "ignores unknown fields" test is exactly the right pin to make schema-evolution intentional.
- **Policy resolver determinism** — `resolve_tool_policy` (`policy.rs:187`) uses `BTreeMap` everywhere so `--output json` is byte-stable. 12 unit tests in the same file pin every precedence pair.
- **Deterministic `signed_archive_round_trip_is_deterministic` test** (`tests/marketplace_signed.rs:269`) explicitly guards against "the signature stops asserting anything" drift if tar header generation ever changes.

## Metrics

- Source files: 12 (`builder.rs`, `error.rs`, `index.rs`, `lib.rs`, `loader.rs`, `manifest.rs`, `marketplace.rs`, `mcp_tools.rs`, `policy.rs`, `remote_marketplace.rs`, `skill_md.rs`, `validator.rs`).
- Lines of code: 5,983 total (largest: `builder.rs` 1,209, `remote_marketplace.rs` 883, `loader.rs` 656).
- CLI subcommands (per `CLAUDE.md` / lib API surface; concrete CLI lives in `agentflow-cli`): 12 contracted — `init`, `install`, `list`, `inspect`, `list-tools`, `run`, `chat`, `test`, `validate`, `index`, `marketplace`, plus the marketplace sub-tree (`marketplace install`, `--require-signature`).
- Test files: 3 integration (`manifest_compat.rs`, `marketplace_signed.rs`, `skills_mcp_integration.rs`) + 9 inline `#[cfg(test)]` modules (every source file except `error.rs`, `lib.rs`, `mcp_tools.rs` has at least basic in-module tests; `mcp_tools.rs` does in fact have 4).
- `unwrap()` / `expect()` in non-test code: **1** — `src/remote_marketplace.rs:47` (`reqwest::Client::builder().no_proxy().build()`, infallibility documented in-line). All other matches live inside `#[cfg(test)] mod tests` blocks (loader, builder, index, marketplace, remote_marketplace, validator, policy, skill_md, mcp_tools).
- TODO/FIXME: **0** in `src/` (the one match in `src/builder.rs:998` is the literal string "TODO" in a doc comment describing a now-closed task). 1 occurrence in `examples/skills/code-reviewer/scripts/analyse.py:50` is the user-skill's own pattern definition.
- Public items missing rustdoc: estimated ~25 (mostly the test-supporting `pub fn` accessors like `SkillRegistryIndex::entries` and `MarketplaceSource::normalized_checksum`, plus the inner `From` impls). The high-traffic surface (`SkillBuilder`, `SkillLoader`, `SkillManifest`, `SkillMd`, `RemoteMarketplaceCache`, `MarketplaceSignatureVerifier`, `resolve_tool_policy`) is well-documented.

## Recommendations (prioritized)

1. **Close the path-traversal hole in manifests (M1)** before any remote-marketplace install path goes live. This is the most likely vector for a "malicious skill" headline.
2. **Plug a real signature verifier and rename the bootstrap one (M2)**. The roadmap already calls for this; ship at least a minisign verifier and gate `marketplace install --require-signature` on it. The current CLI `--require-signature` is a near-tautology.
3. **Bound HTTP downloads with size + ETag/conditional support (M3)**. Cheap, prevents both DoS and bandwidth waste, and unblocks the "checksum-pinned artifact cache" roadmap item.
4. **Flip `os_sandbox` default to true for `script` (M4)** at the next breaking version bump. The opt-out is already wired (P10.4.1 per-tool override), so this is just a default change with a migration note.
5. **Factor MCP governance into one shared function (m1)** to remove drift risk between loader and builder.
6. **Build a `BTreeMap<name → entry>` index on `SkillRegistryIndex::load` (m3)** to make the resolver O(1). Tiny change, removes a future scaling problem.
7. **Stop dropping `license` / `compatibility` (m9)** — preserve them on `SkillManifest` so CLI `skill inspect` and the future marketplace UI can show them.
8. **Cap knowledge file size at validate-time (m4)** to keep persona/system-prompt budgets predictable, then design the "Phase 3" semantic indexing path once that floor is enforced.

End of report.
