//! No-op sandbox backend used on platforms without an enforcing implementation.

use tokio::process::Command;

use crate::capability::Capability;

use super::{SandboxBackend, SandboxEnforcement, SandboxError, SandboxScope};

/// Pass-through backend. `is_enforcing()` returns `false`; callers that
/// require actual enforcement should detect this and refuse to spawn.
pub struct NoopSandboxBackend {
  reason: String,
}

impl NoopSandboxBackend {
  pub fn new(reason: impl Into<String>) -> Self {
    Self {
      reason: reason.into(),
    }
  }
}

impl Default for NoopSandboxBackend {
  fn default() -> Self {
    Self::new("no-op sandbox backend (no enforcement)")
  }
}

impl SandboxBackend for NoopSandboxBackend {
  fn name(&self) -> &'static str {
    "noop"
  }

  fn is_enforcing(&self) -> bool {
    false
  }

  fn enforcement_level(&self) -> SandboxEnforcement {
    SandboxEnforcement::Disabled
  }

  fn wrap_command(
    &self,
    _command: &mut Command,
    _effective_capabilities: &[Capability],
    _scope: &SandboxScope,
  ) -> Result<(), SandboxError> {
    tracing::debug!(reason = %self.reason, "running command without OS sandbox enforcement");
    Ok(())
  }
}
