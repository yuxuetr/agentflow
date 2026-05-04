# Skill / Tool / CLI Permission Merge

This document specifies the **three-way capability merge** used by AgentFlow
to decide which OS-mappable capabilities a tool invocation actually receives
at runtime. The model complements the existing `ToolPermission` /
`ToolPolicy` system documented in [TOOL_PERMISSIONS.md](TOOL_PERMISSIONS.md).

## Capabilities vs. Permissions

[`ToolPermission`](../agentflow-tools/src/tool.rs) is a **declarative** label
attached to tool metadata. It is suitable for human inspection and prompt
descriptions, but is too coarse-grained to drive OS-level enforcement.

[`Capability`](../agentflow-tools/src/capability.rs) is the **runtime-facing**
primitive. Each variant is intended to map onto sandbox profiles
(`sandbox-exec` rules on macOS, seccomp filters / mount namespaces on Linux):

| `Capability` | Stable token | Notes |
|--------------|--------------|-------|
| `FsRead`     | `fs.read`    | Read regular files / directories. |
| `FsWrite`    | `fs.write`   | Create, modify, or delete filesystem entries. |
| `Net`        | `net`        | Open outbound TCP/UDP sockets, perform DNS. |
| `Exec`       | `exec`       | Spawn child processes (`fork` / `exec`). |
| `Env`        | `env`        | Read environment variables beyond a constant inherited set. |

`Capability::from_permission` decomposes a `ToolPermission` value into its
constituent capabilities. `ToolPermission::Mcp` for example expands to
`{Net, Exec}` because stdio MCP servers spawn a subprocess and may also open
network sockets.

## The Three Layers

The merge intersects four sets, in this fixed order:

1. **Tool requires** — what the tool needs to do its job. Returned by
   `Tool::requires_capabilities()`. Default: derived from the tool's
   declared `ToolPermission`s.
2. **Skill security** — `manifest.security.tool_permission_allowlist`
   converted to capabilities and installed via
   `ToolRegistry::with_skill_capabilities(...)`. Empty allowlist → layer is
   permissive (no constraint).
3. **Tool policy** — runtime / admin policy through `ToolPolicy`. The
   capability projection comes from `ToolPolicy::allowed_capabilities()`.
   `ToolPolicy::allow_tools(...)` filters by tool name only and is treated
   as permissive at the capability layer.
4. **CLI flag** — explicit override installed via
   `ToolRegistry::with_cli_capabilities(...)`. Reserved for operator usage
   (e.g. a `--allow-net` / `--deny-exec` flag on `skill run`).

A layer is **permissive** when its `Option` is `None`. A `Some(set)` layer
restricts the running effective set: any required capability not in the
layer's set is dropped.

The three-way merge is the intersection:

```text
required ∩ skill_grant ∩ policy_grant ∩ cli_grant
```

A tool invocation is allowed iff every required capability survives every
layer. CLI overrides cannot **grant** a capability that an earlier layer
denied — they can only further restrict.

## Decision Trace

`EffectiveCapabilities::trace` records one [`CapabilityDecisionEntry`] per
layer:

```jsonc
{
  "tool": "shell",
  "required": ["exec"],
  "effective": ["exec"],
  "denied": [],
  "allowed": true,
  "trace": [
    { "source": "tool_required",  "allowed": ["exec"], "running": ["exec"] },
    { "source": "skill_security", "allowed": ["exec"], "running": ["exec"] },
    { "source": "tool_policy",                      "running": ["exec"] },
    { "source": "cli_flag",                         "running": ["exec"] }
  ]
}
```

When a layer is permissive its `allowed` field is omitted. When a layer
drops capabilities, its `dropped` field lists which.

The full `EffectiveCapabilities` value is emitted as
`AgentEvent::ToolCapabilityDecision` immediately after the existing
`AgentEvent::ToolPolicyDecision`. Trace consumers (replay, TUI, OTel
exporter) can inspect either layer or both.

## Worked Example

A skill manifest declares:

```toml
[[security.tool_permission_allowlist]]
filesystem_read = true
network = true
```

A `ShellTool` is registered. `ShellTool::requires_capabilities()` returns
`[Exec]`. The merge produces:

| Layer            | Allowed              | Dropped | Running   |
|------------------|----------------------|---------|-----------|
| `tool_required`  | `[exec]`             | —       | `[exec]`  |
| `skill_security` | `[fs.read, net]`     | `[exec]`| `[]`      |
| `tool_policy`    | (permissive)         | —       | `[]`      |
| `cli_flag`       | (permissive)         | —       | `[]`      |

The shell call is denied with reason
`tool 'shell' was denied capabilities: exec`. The denial surfaces from
`ToolRegistry::execute` as `ToolError::PolicyDenied` and is recorded both in
`capability_audit_log()` and in the agent event stream.

## Inspection

Operators can preview a skill's effective capability surface without running
it:

```bash
agentflow skill inspect path/to/skill --explain-permissions
```

For each declared tool the command prints the required capabilities, the
effective grant, the dropped capabilities at each layer, and the final
allow/deny verdict.

## Backward Compatibility

The capability layer is **additive** to the existing `ToolPolicy` enforcement:

* `ToolPolicy::evaluate(...)` continues to run first and to populate
  `policy_audit_log()`.
* The capability check runs second. It can deny invocations that the
  permission layer allowed (when a tool's required capabilities exceed the
  skill's grant) and is the entry point for future OS-level enforcement
  (PR-B: `sandbox-exec` / seccomp).

For historical tools that do not override `requires_capabilities()`, the
default implementation derives the capability set from declared
`ToolPermission`s, so existing skills continue to work unchanged.
