use std::path::{Path, PathBuf};

/// Policy controlling what operations built-in tools are allowed to perform.
///
/// The default policy is **restrictive**: only a safe set of read-only shell
/// commands are allowed, and all file / network access must be explicitly
/// unlocked by adding entries to the allow-lists.
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
  /// Allowed shell commands (first token of the command string).
  /// If **empty**, ALL commands are blocked.
  pub allowed_commands: Vec<String>,

  /// Allowed path prefixes for file read/write/list operations.
  /// If **empty**, ALL paths are allowed (permissive mode).
  pub allowed_paths: Vec<PathBuf>,

  /// Allowed host suffixes for HTTP requests (e.g. `"example.com"`).
  /// If **empty**, ALL domains are allowed (permissive mode).
  pub allowed_domains: Vec<String>,

  /// Maximum wall-clock time for a single shell command (seconds).
  pub max_exec_time_secs: u64,

  /// Maximum bytes that FileTool will read in a single operation.
  pub max_file_read_bytes: u64,
}

impl Default for SandboxPolicy {
  fn default() -> Self {
    Self {
      allowed_commands: vec![
        "echo", "cat", "ls", "pwd", "date", "grep", "find", "wc", "head", "tail", "sort", "uniq",
        "cut", "tr", "sed", "awk", "stat", "file",
      ]
      .into_iter()
      .map(String::from)
      .collect(),
      allowed_paths: vec![],
      allowed_domains: vec![],
      max_exec_time_secs: 30,
      max_file_read_bytes: 10 * 1024 * 1024, // 10 MB
    }
  }
}

impl SandboxPolicy {
  /// Build a permissive policy that allows everything.
  /// Use with caution — only in trusted, sandboxed environments.
  pub fn permissive() -> Self {
    Self {
      allowed_commands: vec![], // empty = all allowed in permissive
      allowed_paths: vec![],
      allowed_domains: vec![],
      max_exec_time_secs: 60,
      max_file_read_bytes: 100 * 1024 * 1024,
    }
  }

  /// Check whether a shell command is allowed.
  /// `cmd` should be the first whitespace-separated token.
  pub fn is_command_allowed(&self, cmd: &str) -> bool {
    if self.allowed_commands.is_empty() {
      return true; // permissive
    }
    self.allowed_commands.iter().any(|c| c == cmd)
  }

  /// Check whether a filesystem path is reachable under the policy.
  pub fn is_path_allowed(&self, path: &Path) -> bool {
    if self.allowed_paths.is_empty() {
      return true; // permissive
    }
    self
      .allowed_paths
      .iter()
      .any(|allowed| path.starts_with(allowed))
  }

  /// Check whether an HTTP host is reachable under the policy.
  /// `domain` is the bare hostname (no port, no scheme).
  pub fn is_domain_allowed(&self, domain: &str) -> bool {
    if self.allowed_domains.is_empty() {
      return true; // permissive
    }
    self
      .allowed_domains
      .iter()
      .any(|d| domain == d.as_str() || domain.ends_with(&format!(".{}", d)))
  }
}
