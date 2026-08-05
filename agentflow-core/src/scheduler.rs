use crate::retry::RetryPolicy;
use std::{
  path::PathBuf,
  sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
  },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlowExecutionMode {
  Serial,
  Concurrent,
}

// V1.4: `Eq` dropped (kept `PartialEq`) once `node_retry_policy` was
// added -- `RetryPolicy` only supports `PartialEq` (it nests an `f64`
// backoff multiplier). Nothing in the workspace relied on `Eq`
// specifically (no `HashSet<FlowExecutionConfig>` etc.).
#[derive(Debug, Clone, PartialEq)]
pub struct FlowExecutionConfig {
  pub mode: FlowExecutionMode,
  pub max_concurrency: usize,
  pub fail_fast: bool,
  pub continue_on_skip: bool,
  pub run_base_dir: Option<PathBuf>,
  pub cancellation_token: Option<FlowCancellationToken>,
  /// V1.4: optional Flow-wide timeout applied to every node's execution
  /// (`Standard`/`Map`/`While` alike), inside the executor itself.
  /// `None` (default) is the pre-V1.4 behavior: no Flow-level timeout.
  /// Complements (does not replace) `agentflow-config`'s per-node YAML
  /// `timeout_ms` (T3.1's `TimeoutRetryNode`), which wraps a node
  /// *before* it ever reaches this executor and so has no visibility
  /// into `WorkflowEvent`.
  pub node_timeout_ms: Option<u64>,
  /// V1.4: optional Flow-wide retry policy applied to every node's
  /// execution on a transient failure (network / timeout /
  /// rate-limit-class, per [`RetryPolicy`]'s default classification).
  /// `None` (default) is the pre-V1.4 behavior: no Flow-level retry.
  /// Every attempt emits `WorkflowEvent::RetryAttempt` (see
  /// `FlowExecutor::execute_node_with_reliability` in `flow.rs`), unlike
  /// `agentflow-config`'s per-node YAML `max_retries`, which retries
  /// silently from the executor's point of view.
  pub node_retry_policy: Option<RetryPolicy>,
}

impl FlowExecutionConfig {
  pub fn serial() -> Self {
    Self::default()
  }

  pub fn concurrent(max_concurrency: usize) -> Self {
    Self {
      mode: FlowExecutionMode::Concurrent,
      max_concurrency: max_concurrency.max(1),
      fail_fast: true,
      continue_on_skip: true,
      run_base_dir: None,
      cancellation_token: None,
      node_timeout_ms: None,
      node_retry_policy: None,
    }
  }

  pub fn with_run_base_dir(mut self, run_base_dir: impl Into<PathBuf>) -> Self {
    self.run_base_dir = Some(run_base_dir.into());
    self
  }

  pub fn with_cancellation_token(mut self, token: FlowCancellationToken) -> Self {
    self.cancellation_token = Some(token);
    self
  }

  /// V1.4: opt into a Flow-wide per-node timeout.
  pub fn with_node_timeout_ms(mut self, timeout_ms: u64) -> Self {
    self.node_timeout_ms = Some(timeout_ms);
    self
  }

  /// V1.4: opt into a Flow-wide per-node retry policy.
  pub fn with_node_retry_policy(mut self, policy: RetryPolicy) -> Self {
    self.node_retry_policy = Some(policy);
    self
  }
}

impl Default for FlowExecutionConfig {
  fn default() -> Self {
    Self {
      mode: FlowExecutionMode::Serial,
      max_concurrency: 1,
      fail_fast: true,
      continue_on_skip: true,
      run_base_dir: None,
      cancellation_token: None,
      node_timeout_ms: None,
      node_retry_policy: None,
    }
  }
}

/// Process-local cancellation signal for Flow execution.
#[derive(Debug, Clone)]
pub struct FlowCancellationToken {
  cancelled: Arc<AtomicBool>,
}

impl FlowCancellationToken {
  pub fn new() -> Self {
    Self {
      cancelled: Arc::new(AtomicBool::new(false)),
    }
  }

  pub fn cancel(&self) {
    self.cancelled.store(true, Ordering::SeqCst);
  }

  pub fn is_cancelled(&self) -> bool {
    self.cancelled.load(Ordering::SeqCst)
  }
}

impl Default for FlowCancellationToken {
  fn default() -> Self {
    Self::new()
  }
}

impl PartialEq for FlowCancellationToken {
  fn eq(&self, other: &Self) -> bool {
    Arc::ptr_eq(&self.cancelled, &other.cancelled) || self.is_cancelled() == other.is_cancelled()
  }
}

impl Eq for FlowCancellationToken {}
