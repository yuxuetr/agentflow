//! Plugin execution policy (`P1.8`).
//!
//! The host evaluates a [`PluginPolicy`] before letting a plugin
//! install / spawn proceed. Each [`crate::SecurityProfile`] selects a
//! different default policy via [`PluginPolicy::for_profile`]:
//!
//! - `dev`: sandbox is optional, network is whatever the manifest
//!   declared. Useful for prototyping but never picked by automation.
//! - `local` (default): sandbox required. A `--allow-unsandboxed-plugin`
//!   opt-in lets the operator override on a per-invocation basis.
//! - `production`: sandbox required, signature required, network is
//!   explicit-allow only. The opt-in is rejected.
//!
//! Callers fill in a [`PluginEvaluationInput`] from the plugin
//! manifest + the live runtime state (sandbox backend availability,
//! signature presence, CLI flags). The policy returns a structured
//! [`PluginPolicyDecision`] that the caller logs as a trace event
//! before either spawning the plugin or aborting.

use serde::{Deserialize, Serialize};

use crate::SecurityProfile;

/// Network policy band applied to a plugin invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginNetworkPolicy {
  /// Honour whatever the manifest declared. `dev` and `local` use
  /// this band.
  ManifestAllowed,
  /// The manifest must explicitly list each origin (non-empty
  /// `[plugin.capabilities].network`). `production` uses this band.
  ExplicitAllowOnly,
  /// Reject any plugin that requests network access.
  Denied,
}

impl PluginNetworkPolicy {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::ManifestAllowed => "manifest_allowed",
      Self::ExplicitAllowOnly => "explicit_allow_only",
      Self::Denied => "denied",
    }
  }
}

/// Default plugin execution policy tied to a [`SecurityProfile`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPolicy {
  /// Active security profile (recorded in decisions for trace
  /// correlation).
  pub profile: SecurityProfile,
  /// Hard requirement: the sandbox backend must be active and
  /// enforcing. `dev` sets this to `false`.
  pub require_sandbox: bool,
  /// When `true`, the operator can override [`Self::require_sandbox`]
  /// per-invocation via `--allow-unsandboxed-plugin`. `production`
  /// sets this to `false` so the opt-in is always rejected.
  pub allow_sandbox_disabled_opt_in: bool,
  /// Hard requirement: the plugin archive must carry a verified
  /// signature. `production` is the only band that flips this on
  /// today; signature verification at install time lives in
  /// `agentflow-skills::remote_marketplace`.
  pub require_signature: bool,
  /// Network admission band.
  pub network: PluginNetworkPolicy,
}

impl PluginPolicy {
  /// Profile defaults documented in `docs/TOOL_PERMISSIONS.md`.
  pub fn for_profile(profile: SecurityProfile) -> Self {
    match profile {
      SecurityProfile::Dev => Self {
        profile,
        require_sandbox: false,
        allow_sandbox_disabled_opt_in: true,
        require_signature: false,
        network: PluginNetworkPolicy::ManifestAllowed,
      },
      SecurityProfile::Local => Self {
        profile,
        require_sandbox: true,
        allow_sandbox_disabled_opt_in: true,
        require_signature: false,
        network: PluginNetworkPolicy::ManifestAllowed,
      },
      SecurityProfile::Production => Self {
        profile,
        require_sandbox: true,
        allow_sandbox_disabled_opt_in: false,
        require_signature: true,
        network: PluginNetworkPolicy::ExplicitAllowOnly,
      },
    }
  }

  /// Evaluate the policy against a specific plugin invocation.
  pub fn evaluate(&self, input: &PluginEvaluationInput) -> PluginPolicyDecision {
    let mut deny_reasons: Vec<String> = Vec::new();

    // 1. Sandbox check. The opt-in flag is treated as an operator
    //    *intent*: under `production` we record the deny reason
    //    regardless of the current sandbox state so misuse is caught
    //    even when the host happens to be sandboxed by default. Under
    //    `local` / `dev` the opt-in turns into a "yes, I know" override
    //    that allows the install when the sandbox is not enforcing.
    if self.require_sandbox && !self.allow_sandbox_disabled_opt_in && input.allow_unsandboxed_opt_in
    {
      deny_reasons.push(format!(
        "{} profile refuses --allow-unsandboxed-plugin",
        self.profile
      ));
    }
    let sandbox_satisfied = if !self.require_sandbox {
      true
    } else if input.sandbox_enforcing {
      // Sandbox is already enforcing; the opt-in flag, if present,
      // is informational under permissive profiles and a deny under
      // production (handled above).
      self.allow_sandbox_disabled_opt_in || !input.allow_unsandboxed_opt_in
    } else if self.allow_sandbox_disabled_opt_in && input.allow_unsandboxed_opt_in {
      true
    } else {
      deny_reasons.push(format!(
        "{} profile requires an enforcing sandbox; pass --allow-unsandboxed-plugin to opt in",
        self.profile
      ));
      false
    };

    // 2. Signature check.
    let signature_satisfied = if self.require_signature && !input.has_signature {
      deny_reasons.push(format!(
        "{} profile requires a verified signature on the plugin archive",
        self.profile
      ));
      false
    } else {
      true
    };

    // 3. Network check.
    let network_satisfied = match (self.network, input.network_requested) {
      (PluginNetworkPolicy::Denied, true) => {
        deny_reasons.push(format!(
          "{} profile denies plugin network access",
          self.profile
        ));
        false
      }
      (PluginNetworkPolicy::ExplicitAllowOnly, true) => {
        if input.network_origins_explicit {
          true
        } else {
          deny_reasons.push(format!(
            "{} profile requires explicit network origins in the manifest",
            self.profile
          ));
          false
        }
      }
      _ => true,
    };

    let allowed = sandbox_satisfied && signature_satisfied && network_satisfied;
    PluginPolicyDecision {
      plugin_name: input.plugin_name.clone(),
      profile: self.profile,
      allowed,
      deny_reasons,
      require_sandbox: self.require_sandbox,
      require_signature: self.require_signature,
      network_policy: self.network,
      sandbox_active: input.sandbox_enforcing,
      signature_checked: input.has_signature,
      network_requested: input.network_requested,
      network_origins_explicit: input.network_origins_explicit,
      allow_unsandboxed_opt_in: input.allow_unsandboxed_opt_in,
    }
  }
}

/// Inputs to [`PluginPolicy::evaluate`]. The caller (CLI plugin
/// command or plugin host) builds this from the manifest and the
/// active runtime state.
#[derive(Debug, Clone)]
pub struct PluginEvaluationInput {
  pub plugin_name: String,
  /// `true` if a signature was supplied and verified at install time.
  pub has_signature: bool,
  /// `true` if the active sandbox backend is enforcing (matches
  /// [`crate::sandbox::SandboxEnforcement::Enforcing`]).
  pub sandbox_enforcing: bool,
  /// `true` when the plugin manifest declares any network capability.
  pub network_requested: bool,
  /// `true` when the manifest's network grants are individual
  /// origins (used by `production` `ExplicitAllowOnly`).
  pub network_origins_explicit: bool,
  /// `true` when the operator passed `--allow-unsandboxed-plugin` on
  /// the CLI.
  pub allow_unsandboxed_opt_in: bool,
}

impl PluginEvaluationInput {
  /// Convenience constructor that defaults every gate to the
  /// strictest interpretation. Tests and the host both build up the
  /// input incrementally via the public field set.
  pub fn new(plugin_name: impl Into<String>) -> Self {
    Self {
      plugin_name: plugin_name.into(),
      has_signature: false,
      sandbox_enforcing: false,
      network_requested: false,
      network_origins_explicit: false,
      allow_unsandboxed_opt_in: false,
    }
  }
}

/// Structured decision recorded in the trace and surfaced to the
/// caller. Operators can pretty-print the JSON form to debug policy
/// outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginPolicyDecision {
  pub plugin_name: String,
  pub profile: SecurityProfile,
  pub allowed: bool,
  pub deny_reasons: Vec<String>,
  pub require_sandbox: bool,
  pub require_signature: bool,
  pub network_policy: PluginNetworkPolicy,
  pub sandbox_active: bool,
  pub signature_checked: bool,
  pub network_requested: bool,
  pub network_origins_explicit: bool,
  pub allow_unsandboxed_opt_in: bool,
}

impl PluginPolicyDecision {
  /// Convenience: returns the first deny reason joined into a single
  /// human-readable string. Empty when [`Self::allowed`].
  pub fn deny_reason(&self) -> Option<String> {
    if self.allowed {
      None
    } else if self.deny_reasons.is_empty() {
      Some("plugin policy denied".into())
    } else {
      Some(self.deny_reasons.join("; "))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn input_with_sandbox(name: &str, sandbox: bool) -> PluginEvaluationInput {
    let mut input = PluginEvaluationInput::new(name);
    input.sandbox_enforcing = sandbox;
    input
  }

  #[test]
  fn dev_profile_is_permissive() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Dev);
    let decision = policy.evaluate(&input_with_sandbox("p", false));
    assert!(decision.allowed);
    assert!(decision.deny_reasons.is_empty());
    assert_eq!(
      decision.network_policy,
      PluginNetworkPolicy::ManifestAllowed
    );
  }

  #[test]
  fn local_profile_denies_unsandboxed_plugin_without_opt_in() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Local);
    let decision = policy.evaluate(&input_with_sandbox("p", false));
    assert!(!decision.allowed);
    assert!(
      decision
        .deny_reasons
        .iter()
        .any(|reason| reason.contains("--allow-unsandboxed-plugin"))
    );
  }

  #[test]
  fn local_profile_allows_unsandboxed_with_opt_in() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Local);
    let mut input = input_with_sandbox("p", false);
    input.allow_unsandboxed_opt_in = true;
    let decision = policy.evaluate(&input);
    assert!(decision.allowed);
    assert!(decision.deny_reasons.is_empty());
  }

  #[test]
  fn production_profile_rejects_opt_in_overrides() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let mut input = input_with_sandbox("p", false);
    input.allow_unsandboxed_opt_in = true;
    input.has_signature = true;
    let decision = policy.evaluate(&input);
    assert!(!decision.allowed);
    assert!(
      decision
        .deny_reasons
        .iter()
        .any(|reason| reason.contains("refuses --allow-unsandboxed-plugin"))
    );
  }

  #[test]
  fn production_profile_requires_signature() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let mut input = input_with_sandbox("p", true);
    input.has_signature = false;
    let decision = policy.evaluate(&input);
    assert!(!decision.allowed);
    assert!(
      decision
        .deny_reasons
        .iter()
        .any(|reason| reason.contains("verified signature"))
    );
  }

  #[test]
  fn production_profile_allows_signed_sandboxed_plugin() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let mut input = input_with_sandbox("p", true);
    input.has_signature = true;
    let decision = policy.evaluate(&input);
    assert!(decision.allowed);
    assert!(decision.deny_reasons.is_empty());
  }

  #[test]
  fn production_network_explicit_allow_only_rejects_blanket_network() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let mut input = input_with_sandbox("p", true);
    input.has_signature = true;
    input.network_requested = true;
    input.network_origins_explicit = false;
    let decision = policy.evaluate(&input);
    assert!(!decision.allowed);
    assert!(
      decision
        .deny_reasons
        .iter()
        .any(|reason| reason.contains("explicit network origins"))
    );
  }

  #[test]
  fn production_network_with_explicit_origins_is_admitted() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let mut input = input_with_sandbox("p", true);
    input.has_signature = true;
    input.network_requested = true;
    input.network_origins_explicit = true;
    let decision = policy.evaluate(&input);
    assert!(decision.allowed);
  }

  #[test]
  fn decision_serde_round_trip() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Local);
    let decision = policy.evaluate(&input_with_sandbox("p", false));
    let json = serde_json::to_string(&decision).unwrap();
    let parsed: PluginPolicyDecision = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.plugin_name, "p");
    assert_eq!(parsed.allowed, decision.allowed);
    assert_eq!(parsed.profile, SecurityProfile::Local);
  }

  #[test]
  fn deny_reason_aggregates_messages() {
    let policy = PluginPolicy::for_profile(SecurityProfile::Production);
    let decision = policy.evaluate(&input_with_sandbox("p", false));
    let reason = decision.deny_reason().unwrap();
    assert!(reason.contains("sandbox"));
    assert!(reason.contains("signature"));
  }
}
