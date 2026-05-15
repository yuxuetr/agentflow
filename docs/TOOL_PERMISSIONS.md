# Tool Permission Model

AgentFlow exposes a stable permission model through `ToolMetadata`.
Every `ToolDefinition` now includes:

- `metadata.source`: `builtin`, `script`, `mcp`, or `workflow`
- `metadata.permissions.permissions`: normalized permission strings
- `metadata.idempotency`: `idempotent`, `non_idempotent`, or `unknown`

## Permissions

- `filesystem_read`: read local filesystem state
- `filesystem_write`: write or mutate local filesystem state
- `process_exec`: execute local commands or scripts
- `network`: make outbound network requests
- `mcp`: connect to or invoke MCP servers
- `workflow`: execute nested AgentFlow workflows

## Defaults By Source

- builtin `shell`: `process_exec`
- builtin `file`: `filesystem_read`, `filesystem_write`
- builtin `http`: `network`
- `script`: `process_exec`, `filesystem_read`
- `mcp`: `mcp`, `network`
- `workflow`: `workflow`

The permission model is inspectable today and is intended as the common
surface for future enforcement, audit logging, and policy configuration.

## Idempotency

`ToolMetadata.idempotency` is the static default. Tools whose replay safety
depends on inputs implement `Tool::idempotency(params)`:

- file `read` / `list`: `idempotent`
- file `write`: `non_idempotent`
- HTTP `GET`: `idempotent`
- HTTP `POST`: `non_idempotent`
- shell and script tools: `non_idempotent`
- MCP tools: `unknown` unless the description or schema declares a hint

Agent runtime traces copy known idempotency into `_agentflow.side_effect_class`.
`AgentNode` uses that durable metadata during checkpoint resume: idempotent
unresolved calls can be replayed, while non-idempotent or unknown unresolved
calls require manual recovery.

## HTTP SSRF Protection

`HttpTool` validates every request URL before opening the connection and
validates each redirect destination before following it. Domain allow-lists
still use `SandboxPolicy.allowed_domains`, but network-local destinations are
blocked even when the domain list is empty.

The default policy rejects:

- loopback addresses such as `127.0.0.1`, `::1`, and DNS names resolving to
  them
- link-local addresses such as `169.254.0.0/16` and IPv6 `fe80::/10`
- private addresses such as RFC1918 IPv4 ranges and IPv6 ULA `fc00::/7`
- well-known cloud metadata endpoints such as `169.254.169.254`,
  `100.100.100.200`, and metadata hostnames
- non-HTTP schemes

Trusted callers can opt in explicitly through `SandboxPolicy` fields:
`allow_loopback_network_access`, `allow_link_local_network_access`,
`allow_private_network_access`, and `allow_cloud_metadata_access`.

## OS-Level Sandbox Backends

The permission model above is enforced **in-process** through `ToolPolicy`,
`SandboxPolicy`, and the
[three-way capability merge](SKILL_PERMISSIONS.md). On top of that, an
optional **OS-level layer** wraps `ShellTool` and `ScriptTool` invocations
in a kernel-enforced sandbox so that a bug in the in-process check still
cannot grant a child more authority than the merge allowed.

| Platform | Backend                             | Crate symbol |
|----------|-------------------------------------|--------------|
| macOS    | `sandbox-exec` SBPL profile         | `agentflow_tools::sandbox::MacosSandboxExecBackend` |
| Linux    | seccomp-bpf via `pre_exec`          | `agentflow_tools::sandbox::LinuxSeccompBackend` |
| other    | no-op (rejects when enforcement is required) | `agentflow_tools::sandbox::NoopSandboxBackend` |

`agentflow_tools::sandbox::default_backend()` selects the right backend at
runtime; callers that require real enforcement should check
`SandboxBackend::is_enforcing()` before spawning.

`agentflow doctor` reports the selected backend, whether it is enforcing,
and any sandbox risk warnings. A non-enforcing backend is not silent: the
doctor status becomes `warning` and the JSON output includes a sandbox
warning explaining that subprocesses are protected only by in-process
policy checks.

### Sandbox visibility

The active sandbox backend and its enforcement state are observable
everywhere a shell, script, or plugin tool runs. Operators reading a trace
or a doctor report should never have to guess whether the kernel is
actually constraining a child process.

**Enforcement levels** — `SandboxBackend::enforcement_level()` returns one
of three tokens:

| Token        | Meaning                                                                                  |
|--------------|------------------------------------------------------------------------------------------|
| `enforcing`  | Backend installed and actively constraining the child (macOS with `sandbox-exec`, Linux seccomp on a supported arch). |
| `permissive` | Platform backend exists but cannot enforce in the current environment (missing `sandbox-exec` binary, unsupported Linux arch). Usually points at a misconfiguration. |
| `disabled`   | No enforcing backend is available on this platform (e.g. Windows, or a tool that explicitly opted out via `NoopSandboxBackend`). |

`SandboxBackend::is_enforcing()` remains as a boolean shortcut for legacy
call sites; it returns `true` iff the level is `enforcing`.

**Trace events** — every shell/script/plugin invocation emits an
`AgentEvent::ToolCapabilityDecision` with a `sandbox` field:

```json
{
  "event": "tool_capability_decision",
  "tool": "shell",
  "sandbox": { "backend": "sandbox-exec", "enforcement": "enforcing" }
}
```

In-process tools (HTTP, file, MCP) omit the `sandbox` field because no OS
backend is engaged. The field is `#[serde(skip_serializing_if = "Option::is_none")]`,
so older trace consumers continue to deserialise.

A `noop` backend is **not** silent: the event still records
`{ "backend": "noop", "enforcement": "disabled" }` so misconfigured
shell/script tools are visible in `agentflow trace replay`. This is what
the P1.6 visibility rule enforces: a missing sandbox must always be a
loud condition in traces, never a default omitted from the event stream.

**Doctor output** — `agentflow doctor --output json` returns both the
tri-state `enforcement` token and the legacy `enforcing` boolean:

```json
"sandbox": {
  "backend": "sandbox-exec",
  "enforcement": "enforcing",
  "enforcing": true,
  "capabilities": ["process", "filesystem", "network"],
  "warnings": []
}
```

The two warning shapes differ between `permissive` ("installed but not
enforcing in this environment") and `disabled` ("no enforcing sandbox
backend is available") so an operator reading the report can distinguish
"my platform has no backend" from "the backend is broken on this host".

## Sandbox Matrix Coverage

The regression matrix in `agentflow-tools/tests/sandbox_matrix.rs` covers
the main escape classes that the in-process and OS layers are expected to
handle:

- path traversal such as `allowed/../outside`
- absolute path reads and writes outside the allow-list
- symlink reads that resolve outside the allow-list
- hardlink creation attempts from sandboxed subprocesses when `fs.write`
  was not granted
- unapproved command execution
- timeout handling for long-running subprocesses
- large stdout handling
- policy denials with explicit `deny_reason`

`SandboxPolicy::path_denial_reason` canonicalizes existing paths before
comparing them with allowed prefixes, so symlinks are checked by their
resolved target. For writes to new files, `FileTool` validates both the
requested target and its parent directory before creating directories or
writing content.

`FileTool` also rejects hardlinked regular files by default. Hardlinks do not
carry enough path provenance to prove that an allowed path is the file's only
reachable name, so callers must explicitly opt in with
`SandboxPolicy.allow_hardlinked_files` when that behavior is trusted.

### macOS — `sandbox-exec`

`MacosSandboxExecBackend::wrap_command` writes a TinyScheme (SBPL) profile
to a tempfile and rewrites the command in place to:

```text
/usr/bin/sandbox-exec -f <profile.sb> <original_program> [args...]
```

The profile starts from `(deny default)` plus the minimum rules needed for
any binary to start (dyld init, `process-info*`, root directory stat). It
then layers per-capability grants:

- `Capability::FsRead` → `(allow file-read* (subpath "<read_path>"))` for each scope path.
- `Capability::FsWrite` → `(allow file-write* (subpath "<write_path>"))` for each scope path. Write paths also receive read so round-trip operations like edit-in-place work.
- `Capability::Net` → `(allow network*)`.
- `Capability::Exec` → `(allow process-exec)`.

`SBPL` is officially deprecated by Apple but `/usr/bin/sandbox-exec` ships
on every supported macOS release and is the same primitive Chromium and
Firefox use. If a future macOS removes it, `MacosSandboxExecBackend::new`
detects the missing binary and `wrap_command` returns `Unsupported`; the
tool call fails fast rather than running unsandboxed.

### Linux — seccomp BPF

`LinuxSeccompBackend::wrap_command` compiles a BPF filter via the
[`seccompiler`](https://crates.io/crates/seccompiler) crate, then installs
it through `Command::pre_exec`. The filter runs in the forked child
between `fork(2)` and `execve(2)`; from that point on, every syscall the
child issues is checked.

The default action is `Allow`; the filter only adds rules to **deny**
syscalls that map to capabilities the merge withheld:

| Missing capability | Denied syscalls |
|--------------------|-----------------|
| `Net`              | `socket`, `socketpair`, `connect`, `bind`, `listen`, `accept`, `accept4`, `sendto`, `sendmsg`, `recvfrom`, `recvmsg`, `setsockopt`, `getsockopt`, `getsockname`, `getpeername`, `shutdown` |
| `FsWrite`          | `unlinkat`, `renameat`, `renameat2`, `mkdirat`, `mknodat`, `symlinkat`, `linkat`, `fchmodat`, `fchownat`, `truncate`, `ftruncate` |

Denied syscalls return `EPERM`; the child sees a normal libc error. The
filter targets `x86_64` and `aarch64`. On other Linux architectures the
backend reports itself non-enforcing, and any tool requiring real
enforcement should refuse to spawn.

#### Why `Exec` cannot be a kernel-level deny

The seccomp filter is installed before `execve(2)`, but the very first
syscall the child issues *is* `execve` — to start the requested program.
Globally denying it would block the child from starting at all. Tools that
should not have `Exec` are already denied by the in-process capability
merge before reaching the backend; the kernel does not need a redundant
rule.

#### Why path-scoped FS rules are not enforced

seccomp checks syscall numbers and argument values, not file paths.
Restricting `FsRead` to a specific subtree requires Landlock (Linux 5.13+)
or an LSM. That is out of scope for v0.3.0 — path-prefix enforcement
remains an in-process check via `SandboxPolicy::is_path_allowed`.

### Scope

`SandboxScope` is the per-invocation projection passed to the backend:

```rust
pub struct SandboxScope {
  pub read_paths: Vec<PathBuf>,
  pub write_paths: Vec<PathBuf>,
  pub working_directory: Option<PathBuf>,
}
```

Built-in tools build their own scope:

- **`ShellTool`** uses the merged `SandboxPolicy.allowed_paths` for both
  read and write. When the policy is permissive (empty allowlist) it falls
  back to `/tmp` plus the current working directory; this keeps shell
  builtins working without granting the entire filesystem.
- **`ScriptTool`** always allows reading the skill's `scripts/` directory
  (the script and its sibling resources live there). Additional
  `allowed_paths` from the policy become read+write targets.

### Opting in

The OS layer is **off by default** so legacy skills are not affected. Opt
in per skill:

```toml
# skill.toml
[security]
os_sandbox = true
tool_permission_allowlist = ["filesystem_read", "process_exec"]
```

or in YAML front-matter on a `SKILL.md` file. `SecurityConfig.os_sandbox`
defaults to `false`.

In Rust code (programmatic use):

```rust
use std::sync::Arc;
use agentflow_tools::SandboxPolicy;
use agentflow_tools::builtin::ShellTool;

let policy = Arc::new(SandboxPolicy::permissive());
let tool = ShellTool::new(policy).with_os_sandbox();
// every command is now wrapped via sandbox-exec or seccomp.
```

Tests can substitute a custom backend via `with_backend(...)`; this is how
the integration tests in `agentflow-tools/tests/sandbox_macos.rs` and
`agentflow-tools/tests/sandbox_linux.rs` exercise each path in isolation.

### Failure modes

| Symptom                                                | Likely cause                                |
|--------------------------------------------------------|---------------------------------------------|
| `SandboxViolation: OS sandbox preparation failed: ...` | Backend missing binary (`sandbox-exec` not present) or unsupported arch. Tool refuses to spawn — the in-process layer correctly fails closed. |
| Child exits 134 (SIGABRT) on macOS                      | Profile is missing a baseline rule. The current baseline is sized for normal binary startup; file a bug if you hit this. |
| Child returns `EPERM` from `socket`/`open*` on Linux    | Expected: the missing capability matches a denied syscall. |

## Plugin runtime: same backend, different bridge

The subprocess plugin runtime in `agentflow-core::plugin` reuses the same
backends (`MacosSandboxExecBackend`, `LinuxSeccompBackend`,
`NoopSandboxBackend`) through a thin adapter (`OsSandboxPluginPreparer`,
in `agentflow-cli/src/executor/plugin.rs`). The adapter translates a
plugin manifest's `[plugin.capabilities]` block into the same
`Vec<Capability> + SandboxScope` pair that built-in tools use, then calls
`SandboxBackend::wrap_command` on the spawn `Command`. See
[`docs/PLUGIN_DESIGN.md` §6.5](PLUGIN_DESIGN.md#65-permission-model) for
the full translation table and the `AGENTFLOW_PLUGIN_SANDBOX=1` opt-in
flag. The capability merge layer (skill / policy / CLI) does **not**
apply to plugin spawns — plugins are governed by their own manifest
declarations, not by the host workflow's skill security.

## Plugin policy (P1.8)

`agentflow-tools::PluginPolicy` is the second admission gate for plugins.
Where the sandbox layer above decides *how* a plugin runs once it's been
spawned, the plugin policy decides *whether* the plugin is allowed to be
installed at all under the active security profile. The CLI evaluates
the policy at `agentflow plugin install` time and refuses to write any
files when the decision is `Deny`.

| Profile | Sandbox | Sandbox opt-in | Signature | Network |
| --- | --- | --- | --- | --- |
| `dev` | optional | n/a | not required | manifest-declared origins allowed |
| `local` (default) | required | `--allow-unsandboxed-plugin` honored | not required | manifest-declared origins allowed |
| `production` | required | **rejected** even if supplied | required | only explicit non-wildcard origins admitted |

Behavioral rules:

- The `--allow-unsandboxed-plugin` flag is treated as an operator
  intent. Under `production` the flag is recorded as a deny reason
  even when the sandbox backend happens to be active, so misuse is
  detected before it can land on a host that lacks the sandbox.
- `--signed` tells the install command that the plugin archive
  carried a verified signature (this is the same signal the
  marketplace install path produces). `production` denies any
  install that does not set it.
- A non-empty `[plugin.capabilities].network` array containing
  `*` or an empty string is treated as a wildcard / non-explicit
  grant — `production` rejects it.
- Every decision is logged as `tracing::info!` on the
  `agentflow.plugin.policy` target with the structured fields
  `plugin`, `profile`, `allowed`, `sandbox_active`,
  `signature_checked`, and `network_policy`. Trace replay tools
  can grep for the target name; a typed `WorkflowEvent` variant
  is reserved as a follow-up once enough consumers want it.
