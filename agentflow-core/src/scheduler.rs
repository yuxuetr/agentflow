use std::path::PathBuf;

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
    }
  }

  pub fn with_run_base_dir(mut self, run_base_dir: impl Into<PathBuf>) -> Self {
    self.run_base_dir = Some(run_base_dir.into());
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
    }
  }
}
