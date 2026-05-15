//! Skill-level tool admission policy (`P1.9`).
//!
//! AgentFlow runs tools that come from four different sources: skill
//! manifests, MCP servers attached to the skill, top-level
//! `ToolPolicy` configuration, and CLI overrides supplied at run
//! time. Each layer can either grant or revoke an individual tool,
//! and the order in which their opinions are merged is part of the
//! v1 stability promise.
//!
//! [`resolve_tool_policy`] is the single function that consumes all
//! four layers and produces a [`ResolvedToolPolicy`] — a map of
//! tool name → [`ToolAdmission`] with the resolved verdict and the
//! [`AdmissionSource`] that fired. The CLI surfaces the resolved
//! policy via `agentflow skill inspect --explain-permissions`; agent
//! runtimes consult it before they dispatch any tool call.
//!
//! Precedence (highest first):
//!
//! 1. CLI `--deny-tool` — operator override, always wins.
//! 2. CLI `--allow-tool` — operator override, beats every layer below.
//! 3. SkillSecurity `denied_tools` — manifest-declared deny.
//! 4. SkillSecurity `allowed_tools` — manifest-declared allow.
//! 5. MCP server capability — tool advertised by an MCP server in
//!    `skill_mcp_server_allowlist`.
//! 6. `ToolPolicy` default — top-level policy fall-back.
//!
//! See `docs/MCP_CAPABILITY_POLICY.md` for the rationale and worked
//! examples.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use agentflow_tools::{ToolMetadata, ToolPermissionSet, ToolPolicy, ToolSource};

/// Per-MCP-server capability listing: server name → declared tool
/// names. The host normalises both sides to the wire-level tool name
/// (`mcp__<server>__<tool>`) before merge.
pub type McpCapabilityMap = BTreeMap<String, Vec<String>>;

/// Inputs to [`resolve_tool_policy`]. Each layer is optional; an
/// empty slice (or empty map) means "no constraint at this layer".
#[derive(Debug, Clone)]
pub struct PolicyResolutionInput<'a> {
  /// Universe of known tools. Decisions are emitted for every name
  /// in this set so callers see "allowed" / "denied" rows for tools
  /// the operator might be expecting. Pass the tools you actually
  /// plan to dispatch — typically the union of skill `tools`
  /// declarations and the MCP servers' capability listings.
  pub known_tools: &'a [String],

  /// Tool names declared in the skill manifest's allowed-tools list.
  /// Empty = the skill did not narrow the universe at this layer.
  pub skill_allowed_tools: &'a [String],

  /// Tool names explicitly denied by the skill manifest. Empty = no
  /// skill-level denylist.
  pub skill_denied_tools: &'a [String],

  /// MCP server tools advertised through capability discovery.
  pub mcp_server_capabilities: &'a McpCapabilityMap,

  /// MCP servers the skill explicitly allowlisted. Empty = trust
  /// every server in [`Self::mcp_server_capabilities`].
  pub skill_mcp_server_allowlist: &'a [String],

  /// CLI `--allow-tool` overrides. Beats every layer below but loses
  /// to [`Self::cli_deny_tools`] on a tie.
  pub cli_allow_tools: &'a [String],

  /// CLI `--deny-tool` overrides. Highest precedence.
  pub cli_deny_tools: &'a [String],

  /// Optional [`ToolPolicy`] fall-back. When `None`, tools that
  /// don't match any other layer get [`AdmissionSource::NoMatch`]
  /// with `allowed = false` so callers see an explicit refusal
  /// instead of a silent allow.
  pub fallback_policy: Option<&'a ToolPolicy>,

  /// Metadata for tools that exist in the registry; consumed by the
  /// `ToolPolicy` fall-back evaluation. Keyed by tool name. Tools
  /// without metadata receive a synthetic `ToolMetadata` whose
  /// `source = Builtin` for evaluation purposes.
  pub tool_metadata: &'a BTreeMap<String, ToolMetadata>,
}

impl<'a> PolicyResolutionInput<'a> {
  /// Minimal constructor for tests and ad-hoc use. The empty
  /// defaults match the "no constraint" semantics for every layer.
  pub fn for_tools(known_tools: &'a [String]) -> Self {
    static EMPTY_VEC: Vec<String> = Vec::new();
    static EMPTY_MAP: std::sync::OnceLock<McpCapabilityMap> = std::sync::OnceLock::new();
    static EMPTY_META: std::sync::OnceLock<BTreeMap<String, ToolMetadata>> =
      std::sync::OnceLock::new();
    Self {
      known_tools,
      skill_allowed_tools: &EMPTY_VEC,
      skill_denied_tools: &EMPTY_VEC,
      mcp_server_capabilities: EMPTY_MAP.get_or_init(BTreeMap::new),
      skill_mcp_server_allowlist: &EMPTY_VEC,
      cli_allow_tools: &EMPTY_VEC,
      cli_deny_tools: &EMPTY_VEC,
      fallback_policy: None,
      tool_metadata: EMPTY_META.get_or_init(BTreeMap::new),
    }
  }
}

/// Which layer fired to decide a tool's admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionSource {
  CliDeny,
  CliAllow,
  SkillDeny,
  SkillAllow,
  McpServerCapability,
  ToolPolicyDefault,
  /// No layer matched. With [`PolicyResolutionInput::fallback_policy`]
  /// set to `Some(allow_all)` this never fires; without it, unmatched
  /// tools default to `allowed = false` so misconfiguration surfaces
  /// as a deny rather than a silent allow.
  NoMatch,
}

impl AdmissionSource {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::CliDeny => "cli_deny",
      Self::CliAllow => "cli_allow",
      Self::SkillDeny => "skill_deny",
      Self::SkillAllow => "skill_allow",
      Self::McpServerCapability => "mcp_server_capability",
      Self::ToolPolicyDefault => "tool_policy_default",
      Self::NoMatch => "no_match",
    }
  }
}

/// Single resolved admission for one tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolAdmission {
  pub tool: String,
  pub allowed: bool,
  pub source: AdmissionSource,
  pub reason: String,
  /// MCP server that advertised the tool, when [`AdmissionSource::McpServerCapability`]
  /// fired. `None` for every other source.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub mcp_server: Option<String>,
}

/// Aggregated policy decision for a set of known tools.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ResolvedToolPolicy {
  /// Decisions keyed by tool name (`BTreeMap` gives stable iteration
  /// order so `--output json` is reproducible).
  pub decisions: BTreeMap<String, ToolAdmission>,
}

impl ResolvedToolPolicy {
  pub fn allow_count(&self) -> usize {
    self.decisions.values().filter(|a| a.allowed).count()
  }

  pub fn deny_count(&self) -> usize {
    self.decisions.values().filter(|a| !a.allowed).count()
  }

  pub fn get(&self, tool: &str) -> Option<&ToolAdmission> {
    self.decisions.get(tool)
  }

  pub fn iter(&self) -> impl Iterator<Item = (&String, &ToolAdmission)> {
    self.decisions.iter()
  }
}

/// Merge every policy layer into a single set of per-tool decisions.
///
/// Each tool name in [`PolicyResolutionInput::known_tools`] produces
/// exactly one [`ToolAdmission`]. The precedence order (CLI deny >
/// CLI allow > SkillDeny > SkillAllow > McpServerCapability >
/// ToolPolicyDefault) is documented in
/// `docs/MCP_CAPABILITY_POLICY.md` and locked down by the tests at
/// the bottom of this module.
pub fn resolve_tool_policy(input: PolicyResolutionInput<'_>) -> ResolvedToolPolicy {
  let cli_deny: BTreeSet<&str> = input.cli_deny_tools.iter().map(|s| s.as_str()).collect();
  let cli_allow: BTreeSet<&str> = input.cli_allow_tools.iter().map(|s| s.as_str()).collect();
  let skill_deny: BTreeSet<&str> = input
    .skill_denied_tools
    .iter()
    .map(|s| s.as_str())
    .collect();
  let skill_allow: BTreeSet<&str> = input
    .skill_allowed_tools
    .iter()
    .map(|s| s.as_str())
    .collect();
  let mcp_allowlist: Option<BTreeSet<&str>> = if input.skill_mcp_server_allowlist.is_empty() {
    None
  } else {
    Some(
      input
        .skill_mcp_server_allowlist
        .iter()
        .map(|s| s.as_str())
        .collect(),
    )
  };

  let mut decisions: BTreeMap<String, ToolAdmission> = BTreeMap::new();
  for tool in input.known_tools {
    let admission = resolve_one(
      tool,
      &cli_deny,
      &cli_allow,
      &skill_deny,
      &skill_allow,
      input.mcp_server_capabilities,
      mcp_allowlist.as_ref(),
      input.fallback_policy,
      input.tool_metadata.get(tool),
    );
    decisions.insert(tool.clone(), admission);
  }

  ResolvedToolPolicy { decisions }
}

#[allow(clippy::too_many_arguments)]
fn resolve_one(
  tool: &str,
  cli_deny: &BTreeSet<&str>,
  cli_allow: &BTreeSet<&str>,
  skill_deny: &BTreeSet<&str>,
  skill_allow: &BTreeSet<&str>,
  mcp_caps: &McpCapabilityMap,
  mcp_allowlist: Option<&BTreeSet<&str>>,
  fallback: Option<&ToolPolicy>,
  metadata: Option<&ToolMetadata>,
) -> ToolAdmission {
  if cli_deny.contains(tool) {
    return admission(
      tool,
      false,
      AdmissionSource::CliDeny,
      "denied by --deny-tool",
    );
  }
  if cli_allow.contains(tool) {
    return admission(
      tool,
      true,
      AdmissionSource::CliAllow,
      "allowed by --allow-tool",
    );
  }
  if skill_deny.contains(tool) {
    return admission(
      tool,
      false,
      AdmissionSource::SkillDeny,
      "denied by skill manifest",
    );
  }
  if skill_allow.contains(tool) {
    return admission(
      tool,
      true,
      AdmissionSource::SkillAllow,
      "allowed by skill manifest",
    );
  }
  for (server, tools) in mcp_caps {
    if !tools.iter().any(|t| t == tool) {
      continue;
    }
    let server_allowed = mcp_allowlist
      .map(|allow| allow.contains(server.as_str()))
      .unwrap_or(true);
    if server_allowed {
      let mut adm = admission(
        tool,
        true,
        AdmissionSource::McpServerCapability,
        format!("advertised by MCP server '{server}'").as_str(),
      );
      adm.mcp_server = Some(server.clone());
      return adm;
    }
  }
  if let Some(policy) = fallback {
    let synthetic;
    let meta_ref = if let Some(m) = metadata {
      m
    } else {
      synthetic = ToolMetadata {
        source: ToolSource::Builtin,
        permissions: ToolPermissionSet::default(),
        idempotency: agentflow_tools::ToolIdempotency::Unknown,
        mcp_server_name: None,
        mcp_tool_name: None,
      };
      &synthetic
    };
    let decision = policy.evaluate(tool, meta_ref, &serde_json::Value::Null);
    return admission(
      tool,
      decision.allowed,
      AdmissionSource::ToolPolicyDefault,
      decision
        .deny_reason
        .as_deref()
        .unwrap_or("matched top-level ToolPolicy default"),
    );
  }
  admission(
    tool,
    false,
    AdmissionSource::NoMatch,
    "no layer matched and no fallback ToolPolicy supplied; treating as deny",
  )
}

fn admission(tool: &str, allowed: bool, source: AdmissionSource, reason: &str) -> ToolAdmission {
  ToolAdmission {
    tool: tool.to_owned(),
    allowed,
    source,
    reason: reason.to_owned(),
    mcp_server: None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn names(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| s.to_string()).collect()
  }

  fn mcp_map(entries: &[(&str, &[&str])]) -> McpCapabilityMap {
    entries
      .iter()
      .map(|(server, tools)| {
        (
          (*server).to_string(),
          tools.iter().map(|t| (*t).to_string()).collect(),
        )
      })
      .collect()
  }

  #[test]
  fn cli_deny_overrides_everything() {
    let known = names(&["shell"]);
    let allow = names(&["shell"]);
    let deny = names(&["shell"]);
    let skill_allow = names(&["shell"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.cli_allow_tools = &allow;
    input.cli_deny_tools = &deny;
    input.skill_allowed_tools = &skill_allow;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("shell").unwrap();
    assert!(!adm.allowed);
    assert_eq!(adm.source, AdmissionSource::CliDeny);
  }

  #[test]
  fn cli_allow_beats_skill_deny() {
    let known = names(&["shell"]);
    let cli_allow = names(&["shell"]);
    let skill_deny = names(&["shell"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.cli_allow_tools = &cli_allow;
    input.skill_denied_tools = &skill_deny;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("shell").unwrap();
    assert!(adm.allowed);
    assert_eq!(adm.source, AdmissionSource::CliAllow);
  }

  #[test]
  fn skill_deny_beats_skill_allow_and_mcp() {
    let known = names(&["read_doc"]);
    let skill_deny = names(&["read_doc"]);
    let skill_allow = names(&["read_doc"]);
    let mcp = mcp_map(&[("docs", &["read_doc"])]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.skill_denied_tools = &skill_deny;
    input.skill_allowed_tools = &skill_allow;
    input.mcp_server_capabilities = &mcp;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("read_doc").unwrap();
    assert!(!adm.allowed);
    assert_eq!(adm.source, AdmissionSource::SkillDeny);
  }

  #[test]
  fn skill_allow_beats_mcp_advertisement() {
    let known = names(&["search"]);
    let skill_allow = names(&["search"]);
    let mcp = mcp_map(&[("knowledge", &["search"])]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.skill_allowed_tools = &skill_allow;
    input.mcp_server_capabilities = &mcp;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("search").unwrap();
    assert!(adm.allowed);
    assert_eq!(adm.source, AdmissionSource::SkillAllow);
    assert!(adm.mcp_server.is_none());
  }

  #[test]
  fn mcp_server_capability_grants_unknown_tool() {
    let known = names(&["search"]);
    let mcp = mcp_map(&[("knowledge", &["search"])]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.mcp_server_capabilities = &mcp;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("search").unwrap();
    assert!(adm.allowed);
    assert_eq!(adm.source, AdmissionSource::McpServerCapability);
    assert_eq!(adm.mcp_server.as_deref(), Some("knowledge"));
  }

  #[test]
  fn mcp_server_capability_respects_skill_allowlist() {
    let known = names(&["search"]);
    let mcp = mcp_map(&[("knowledge", &["search"]), ("shadow", &["search"])]);
    let allowlist = names(&["knowledge"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.mcp_server_capabilities = &mcp;
    input.skill_mcp_server_allowlist = &allowlist;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("search").unwrap();
    assert!(adm.allowed);
    assert_eq!(adm.source, AdmissionSource::McpServerCapability);
    assert_eq!(adm.mcp_server.as_deref(), Some("knowledge"));
  }

  #[test]
  fn mcp_server_not_in_allowlist_falls_through_to_no_match() {
    let known = names(&["search"]);
    let mcp = mcp_map(&[("shadow", &["search"])]);
    let allowlist = names(&["knowledge"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.mcp_server_capabilities = &mcp;
    input.skill_mcp_server_allowlist = &allowlist;
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("search").unwrap();
    assert!(!adm.allowed);
    assert_eq!(adm.source, AdmissionSource::NoMatch);
  }

  #[test]
  fn fallback_tool_policy_resolves_when_higher_layers_silent() {
    let known = names(&["http"]);
    let fallback = ToolPolicy::allow_tools(["http"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.fallback_policy = Some(&fallback);
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("http").unwrap();
    assert!(adm.allowed);
    assert_eq!(adm.source, AdmissionSource::ToolPolicyDefault);
  }

  #[test]
  fn fallback_tool_policy_deny_is_recorded() {
    let known = names(&["dangerous"]);
    let fallback = ToolPolicy::allow_tools(["safe"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.fallback_policy = Some(&fallback);
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("dangerous").unwrap();
    assert!(!adm.allowed);
    assert_eq!(adm.source, AdmissionSource::ToolPolicyDefault);
    assert!(adm.reason.contains("not in the allowlist"));
  }

  #[test]
  fn unmatched_tool_without_fallback_defaults_to_deny() {
    let known = names(&["lonely"]);
    let input = PolicyResolutionInput::for_tools(&known);
    let resolved = resolve_tool_policy(input);
    let adm = resolved.get("lonely").unwrap();
    assert!(!adm.allowed);
    assert_eq!(adm.source, AdmissionSource::NoMatch);
  }

  #[test]
  fn resolved_policy_serializes_round_trip() {
    let known = names(&["shell"]);
    let allow = names(&["shell"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.cli_allow_tools = &allow;
    let resolved = resolve_tool_policy(input);
    let json = serde_json::to_string(&resolved).unwrap();
    let parsed: ResolvedToolPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, resolved);
  }

  #[test]
  fn allow_and_deny_counters_reflect_decisions() {
    let known = names(&["a", "b", "c", "d"]);
    let allow = names(&["a", "b"]);
    let deny = names(&["c"]);
    let mut input = PolicyResolutionInput::for_tools(&known);
    input.cli_allow_tools = &allow;
    input.cli_deny_tools = &deny;
    let resolved = resolve_tool_policy(input);
    assert_eq!(resolved.allow_count(), 2);
    assert_eq!(resolved.deny_count(), 2); // c denied + d no_match
  }
}
