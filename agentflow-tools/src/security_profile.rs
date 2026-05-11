use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

use crate::{Capability, ToolPermission};

/// Environment variable used by CLI/server entry points to select a profile.
pub const SECURITY_PROFILE_ENV: &str = "AGENTFLOW_SECURITY_PROFILE";

/// Coarse runtime posture for local tools, server routes, plugins, and remote
/// marketplace operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityProfile {
  /// Trusted development: fastest feedback loop, intentionally permissive.
  Dev,
  /// Default single-user local posture: explicit enough to inspect without
  /// breaking historical local workflows.
  Local,
  /// Shared or exposed deployment posture: fail closed where possible.
  Production,
}

impl Default for SecurityProfile {
  fn default() -> Self {
    Self::Local
  }
}

impl SecurityProfile {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::Dev => "dev",
      Self::Local => "local",
      Self::Production => "production",
    }
  }

  pub fn from_env() -> Result<Self, SecurityProfileError> {
    match std::env::var(SECURITY_PROFILE_ENV) {
      Ok(value) => value.parse(),
      Err(std::env::VarError::NotPresent) => Ok(Self::default()),
      Err(std::env::VarError::NotUnicode(_)) => Err(SecurityProfileError::NotUnicode),
    }
  }

  pub fn defaults(self) -> SecurityProfileDefaults {
    match self {
      Self::Dev => SecurityProfileDefaults {
        profile: self,
        auth: AuthDefaults {
          require_api_token: false,
          allow_unauthenticated_loopback: true,
        },
        cors: CorsDefaults {
          mode: CorsMode::Permissive,
          allowed_origins: Vec::new(),
        },
        request_limits: RequestLimitDefaults {
          max_request_body_bytes: 100 * 1024 * 1024,
          max_workflow_submit_bytes: 10 * 1024 * 1024,
          max_skill_run_bytes: 5 * 1024 * 1024,
        },
        tool_permissions: ToolPermissionDefaults {
          default_permissions: vec![
            ToolPermission::FilesystemRead,
            ToolPermission::FilesystemWrite,
            ToolPermission::ProcessExec,
            ToolPermission::Network,
            ToolPermission::Mcp,
            ToolPermission::Workflow,
          ],
          default_capabilities: vec![
            Capability::FsRead,
            Capability::FsWrite,
            Capability::Exec,
            Capability::Net,
            Capability::Env,
          ],
        },
        sandboxing: SandboxingDefaults {
          require_os_sandbox: false,
          allow_noop_backend: true,
        },
        plugins: PluginExecutionDefaults {
          allow_subprocess_plugins: true,
          require_os_sandbox: false,
          allow_sandbox_disabled_opt_in: true,
        },
        marketplace: MarketplaceInstallDefaults {
          allow_remote_installs: true,
          require_signature_verification: false,
          allow_unsigned_local_fixtures: true,
        },
      },
      Self::Local => SecurityProfileDefaults {
        profile: self,
        auth: AuthDefaults {
          require_api_token: false,
          allow_unauthenticated_loopback: true,
        },
        cors: CorsDefaults {
          mode: CorsMode::Permissive,
          allowed_origins: Vec::new(),
        },
        request_limits: RequestLimitDefaults {
          max_request_body_bytes: 25 * 1024 * 1024,
          max_workflow_submit_bytes: 5 * 1024 * 1024,
          max_skill_run_bytes: 2 * 1024 * 1024,
        },
        tool_permissions: ToolPermissionDefaults {
          default_permissions: vec![
            ToolPermission::FilesystemRead,
            ToolPermission::FilesystemWrite,
            ToolPermission::ProcessExec,
            ToolPermission::Network,
            ToolPermission::Mcp,
            ToolPermission::Workflow,
          ],
          default_capabilities: vec![
            Capability::FsRead,
            Capability::FsWrite,
            Capability::Exec,
            Capability::Net,
            Capability::Env,
          ],
        },
        sandboxing: SandboxingDefaults {
          require_os_sandbox: false,
          allow_noop_backend: true,
        },
        plugins: PluginExecutionDefaults {
          allow_subprocess_plugins: true,
          require_os_sandbox: false,
          allow_sandbox_disabled_opt_in: true,
        },
        marketplace: MarketplaceInstallDefaults {
          allow_remote_installs: true,
          require_signature_verification: true,
          allow_unsigned_local_fixtures: true,
        },
      },
      Self::Production => SecurityProfileDefaults {
        profile: self,
        auth: AuthDefaults {
          require_api_token: true,
          allow_unauthenticated_loopback: false,
        },
        cors: CorsDefaults {
          mode: CorsMode::ExplicitOrigins,
          allowed_origins: Vec::new(),
        },
        request_limits: RequestLimitDefaults {
          max_request_body_bytes: 10 * 1024 * 1024,
          max_workflow_submit_bytes: 1024 * 1024,
          max_skill_run_bytes: 1024 * 1024,
        },
        tool_permissions: ToolPermissionDefaults {
          default_permissions: vec![ToolPermission::FilesystemRead, ToolPermission::Workflow],
          default_capabilities: vec![Capability::FsRead],
        },
        sandboxing: SandboxingDefaults {
          require_os_sandbox: true,
          allow_noop_backend: false,
        },
        plugins: PluginExecutionDefaults {
          allow_subprocess_plugins: false,
          require_os_sandbox: true,
          allow_sandbox_disabled_opt_in: false,
        },
        marketplace: MarketplaceInstallDefaults {
          allow_remote_installs: true,
          require_signature_verification: true,
          allow_unsigned_local_fixtures: false,
        },
      },
    }
  }
}

impl fmt::Display for SecurityProfile {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(self.as_str())
  }
}

impl FromStr for SecurityProfile {
  type Err = SecurityProfileError;

  fn from_str(value: &str) -> Result<Self, Self::Err> {
    match value.trim().to_ascii_lowercase().as_str() {
      "dev" | "development" => Ok(Self::Dev),
      "local" => Ok(Self::Local),
      "prod" | "production" => Ok(Self::Production),
      other => Err(SecurityProfileError::Unknown(other.to_string())),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecurityProfileError {
  #[error("unknown security profile '{0}' (expected dev, local, or production)")]
  Unknown(String),
  #[error("{SECURITY_PROFILE_ENV} is not valid unicode")]
  NotUnicode,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityProfileDefaults {
  pub profile: SecurityProfile,
  pub auth: AuthDefaults,
  pub cors: CorsDefaults,
  pub request_limits: RequestLimitDefaults,
  pub tool_permissions: ToolPermissionDefaults,
  pub sandboxing: SandboxingDefaults,
  pub plugins: PluginExecutionDefaults,
  pub marketplace: MarketplaceInstallDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthDefaults {
  pub require_api_token: bool,
  pub allow_unauthenticated_loopback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorsDefaults {
  pub mode: CorsMode,
  pub allowed_origins: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CorsMode {
  Permissive,
  ExplicitOrigins,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestLimitDefaults {
  pub max_request_body_bytes: u64,
  pub max_workflow_submit_bytes: u64,
  pub max_skill_run_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolPermissionDefaults {
  pub default_permissions: Vec<ToolPermission>,
  pub default_capabilities: Vec<Capability>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SandboxingDefaults {
  pub require_os_sandbox: bool,
  pub allow_noop_backend: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginExecutionDefaults {
  pub allow_subprocess_plugins: bool,
  pub require_os_sandbox: bool,
  pub allow_sandbox_disabled_opt_in: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketplaceInstallDefaults {
  pub allow_remote_installs: bool,
  pub require_signature_verification: bool,
  pub allow_unsigned_local_fixtures: bool,
}

#[cfg(test)]
mod tests {
  use super::{CorsMode, SecurityProfile};
  use crate::{Capability, ToolPermission};

  #[test]
  fn parses_profile_aliases() {
    assert_eq!(
      "dev".parse::<SecurityProfile>().unwrap(),
      SecurityProfile::Dev
    );
    assert_eq!(
      "development".parse::<SecurityProfile>().unwrap(),
      SecurityProfile::Dev
    );
    assert_eq!(
      "prod".parse::<SecurityProfile>().unwrap(),
      SecurityProfile::Production
    );
    assert!("staging".parse::<SecurityProfile>().is_err());
  }

  #[test]
  fn local_is_backward_compatible_default() {
    let defaults = SecurityProfile::default().defaults();
    assert_eq!(defaults.profile, SecurityProfile::Local);
    assert!(!defaults.auth.require_api_token);
    assert_eq!(defaults.cors.mode, CorsMode::Permissive);
    assert!(!defaults.sandboxing.require_os_sandbox);
    assert!(defaults.plugins.allow_subprocess_plugins);
    assert!(defaults.marketplace.allow_remote_installs);
    assert!(
      defaults
        .tool_permissions
        .default_permissions
        .contains(&ToolPermission::ProcessExec)
    );
    assert!(
      defaults
        .tool_permissions
        .default_capabilities
        .contains(&Capability::Exec)
    );
  }

  #[test]
  fn production_defaults_fail_closed_for_exposed_runtime() {
    let defaults = SecurityProfile::Production.defaults();
    assert!(defaults.auth.require_api_token);
    assert_eq!(defaults.cors.mode, CorsMode::ExplicitOrigins);
    assert!(defaults.sandboxing.require_os_sandbox);
    assert!(!defaults.sandboxing.allow_noop_backend);
    assert!(!defaults.plugins.allow_subprocess_plugins);
    assert!(defaults.marketplace.require_signature_verification);
    assert!(!defaults.marketplace.allow_unsigned_local_fixtures);
    assert!(
      !defaults
        .tool_permissions
        .default_capabilities
        .contains(&Capability::Exec)
    );
  }

  #[test]
  fn defaults_are_json_serializable() {
    let value = serde_json::to_value(SecurityProfile::Production.defaults()).unwrap();
    assert_eq!(value["profile"], "production");
    assert_eq!(value["auth"]["require_api_token"], true);
    assert_eq!(value["cors"]["mode"], "explicit_origins");
  }
}
