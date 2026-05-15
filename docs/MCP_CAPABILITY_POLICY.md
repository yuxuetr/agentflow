# MCP Capability & SkillSecurity Merge Policy

Status: stable as of `P1.9`.
Crate: `agentflow-skills`. Module: `agentflow_skills::policy`.
Entry point: [`resolve_tool_policy`](../agentflow-skills/src/policy.rs).

AgentFlow tools can come from four different sources, and each source
has its own opinion about whether a given tool should be admitted into
the running agent's `ToolRegistry`. This document is the v1
contract for how those opinions are merged into a single decision
per tool.

## Layers

1. **CLI overrides** (`--allow-tool` / `--deny-tool`). Operator-supplied
   at run time. Highest precedence because the operator is exercising
   conscious intent and may need to override the static skill manifest
   to debug or to lock down a single invocation.
2. **SkillSecurity** declared in the skill manifest. Two related
   surfaces:
   - The `allowed_tools` list (built-ins and MCP tools the skill
     authors signed off on).
   - The `denied_tools` list (explicit deny that beats `allowed_tools`
     for the same tool name).
3. **MCP server capabilities** discovered at runtime. An MCP server
   advertises a set of tools when the client connects. Each
   advertised tool counts as an implicit grant *only* if the
   server's name is in the skill's `mcp_server_allowlist` (or the
   allowlist is empty, which means "trust every declared server").
4. **`ToolPolicy` default** — the top-level
   `agentflow_tools::ToolPolicy` configured by the platform. This is
   the catch-all that says "permit / deny everything else"; tools
   that never reach this layer never get a chance to bypass it.

## Precedence table

The merge always picks the *first* layer in this order that has an
opinion about the tool:

| # | Layer | Source | Wins over |
| - | --- | --- | --- |
| 1 | CLI `--deny-tool` | runtime operator intent | every layer below |
| 2 | CLI `--allow-tool` | runtime operator intent | every layer below except CLI deny |
| 3 | SkillSecurity `denied_tools` | static skill manifest | every layer below |
| 4 | SkillSecurity `allowed_tools` | static skill manifest | MCP + `ToolPolicy` |
| 5 | MCP server capability | runtime MCP advertise | `ToolPolicy` only |
| 6 | `ToolPolicy` default | top-level platform policy | nothing (fall-through) |

When no layer matches, the resolver records `AdmissionSource::NoMatch`
with `allowed = false`. This is deliberately fail-closed: an
unmatched tool is treated as denied rather than silently allowed.

## `resolve_tool_policy`

```rust
use agentflow_skills::{
  AdmissionSource, PolicyResolutionInput, ResolvedToolPolicy, resolve_tool_policy,
};

let known = vec!["shell".to_string(), "search".to_string()];
let mut input = PolicyResolutionInput::for_tools(&known);
input.skill_allowed_tools = &["shell".to_string()][..];
input.mcp_server_capabilities = &mcp_caps;
input.cli_deny_tools = &["search".to_string()][..];

let resolved: ResolvedToolPolicy = resolve_tool_policy(input);
for (tool, admission) in resolved.iter() {
  println!(
    "{tool}: {} via {}",
    if admission.allowed { "ALLOW" } else { "DENY" },
    admission.source.as_str(),
  );
}
```

`PolicyResolutionInput` carries:

- `known_tools: &[String]` — universe of names to evaluate.
- `skill_allowed_tools: &[String]`,
  `skill_denied_tools: &[String]` — manifest layer.
- `mcp_server_capabilities: &McpCapabilityMap` — `server_name → Vec<tool>`.
- `skill_mcp_server_allowlist: &[String]` — empty = trust every
  declared server.
- `cli_allow_tools: &[String]`, `cli_deny_tools: &[String]` —
  operator overrides.
- `fallback_policy: Option<&ToolPolicy>` — top-level default.
- `tool_metadata: &BTreeMap<String, ToolMetadata>` — fed into the
  fallback policy's permission allow-list check.

`ResolvedToolPolicy` is a `BTreeMap`-backed map keyed by tool name.
Iteration order is stable so `--output json` is reproducible across
runs.

## Worked examples

### 1 · CLI deny wins over a wide-open skill

```text
known: ["shell"]
skill_allowed_tools: ["shell"]
cli_allow_tools: ["shell"]
cli_deny_tools: ["shell"]
→ shell: DENY (cli_deny)
```

### 2 · Skill deny beats MCP advertisement

```text
known: ["read_doc"]
skill_denied_tools: ["read_doc"]
mcp_server_capabilities: { "docs": ["read_doc"] }
→ read_doc: DENY (skill_deny)
```

### 3 · MCP server allowlist filters advertised tools

```text
known: ["search"]
mcp_server_capabilities: { "knowledge": ["search"], "shadow": ["search"] }
skill_mcp_server_allowlist: ["knowledge"]
→ search: ALLOW (mcp_server_capability, server="knowledge")
```

### 4 · Top-level `ToolPolicy` is the fall-back

```text
known: ["http"]
fallback_policy: ToolPolicy::allow_tools(["http"])
→ http: ALLOW (tool_policy_default)
```

### 5 · No layer matched → fail-closed deny

```text
known: ["lonely"]
fallback_policy: None
→ lonely: DENY (no_match)
```

## CLI surface

The CLI (`agentflow skill inspect --explain-permissions`) is the
human-facing view of the resolved policy. The `--allow-tool` and
`--deny-tool` flags are runtime overrides accepted by the same
commands that load a skill (most prominently `agentflow skill run` /
`agentflow harness run --skill`). The merge is identical between CLI
mode and SDK callers because they share the same
[`resolve_tool_policy`] entry point.

## Stability

The function signature, the precedence table, the
`AdmissionSource` enum, and the `ToolAdmission` shape are
**stable** as of v0.4.x. Additions are allowed (new layers, new
fields with serde defaults) but reordering precedence or removing
an `AdmissionSource` variant is a wire-breaking change.
