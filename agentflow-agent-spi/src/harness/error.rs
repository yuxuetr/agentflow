//! Error type shared by Harness Mode contracts.
//!
//! Kept narrow on purpose: Phase H0 only needs a stable error surface
//! that hook / approval / context provider implementations can return.
//! Phase H1 will add runtime-execution error categories.

use thiserror::Error;

/// Errors surfaced by Harness Mode contract types.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum HarnessError {
  /// An [`crate::ApprovalProvider`] explicitly denied a request.
  #[error("approval denied: {0}")]
  ApprovalDenied(String),

  /// An [`crate::ApprovalProvider`] timed out before a decision arrived.
  #[error("approval timed out after {timeout_ms} ms")]
  ApprovalTimeout {
    /// The configured wait window in milliseconds.
    timeout_ms: u64,
  },

  /// A [`crate::PreToolHook`] or [`crate::PostToolHook`] returned an
  /// error. The runtime decides whether to abort the step or continue,
  /// based on hook configuration.
  #[error("hook '{hook}' failed: {message}")]
  HookFailed {
    /// Hook name registered with the runtime.
    hook: String,
    /// Operator-readable failure reason.
    message: String,
  },

  /// A [`crate::ContextProvider`] failed to collect items. The provider
  /// name is preserved so the runtime can record a partial-context event.
  #[error("context provider '{provider}' failed: {message}")]
  ContextProviderFailed {
    /// Provider name as returned by [`crate::ContextProvider::name`].
    provider: String,
    /// Operator-readable failure reason.
    message: String,
  },

  /// The Harness session id was not found in the session store. Used by
  /// resume / inspect entry points.
  #[error("harness session not found: {0}")]
  SessionNotFound(String),

  /// The session is in a state incompatible with the requested
  /// operation (e.g. attempting to resume a completed session).
  #[error("harness session in invalid state: {0}")]
  InvalidState(String),

  /// An envelope failed to parse or serialize; should be rare because
  /// the contract types are serde-managed.
  #[error("harness envelope error: {0}")]
  Envelope(String),

  /// Catch-all for runtime-internal errors the contract layer surfaces
  /// to callers. Implementations should prefer the typed variants above
  /// when possible.
  #[error("harness internal error: {0}")]
  Other(String),
}

impl HarnessError {
  /// Convenience constructor for [`HarnessError::HookFailed`].
  pub fn hook(name: impl Into<String>, message: impl Into<String>) -> Self {
    Self::HookFailed {
      hook: name.into(),
      message: message.into(),
    }
  }

  /// Convenience constructor for [`HarnessError::ContextProviderFailed`].
  pub fn context(provider: impl Into<String>, message: impl Into<String>) -> Self {
    Self::ContextProviderFailed {
      provider: provider.into(),
      message: message.into(),
    }
  }
}
