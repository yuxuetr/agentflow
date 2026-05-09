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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlowExecutionConfig {
  pub mode: FlowExecutionMode,
  pub max_concurrency: usize,
  pub fail_fast: bool,
  pub continue_on_skip: bool,
  pub run_base_dir: Option<PathBuf>,
  pub cancellation_token: Option<FlowCancellationToken>,
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
