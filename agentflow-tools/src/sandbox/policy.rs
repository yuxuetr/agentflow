use std::path::{Component, Path, PathBuf};

/// Network destinations that are denied unless a policy explicitly opts in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkAddressClass {
  Loopback,
  LinkLocal,
  Private,
  CloudMetadata,
}

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

  /// Allow HTTP tools to reach loopback addresses such as `127.0.0.1` and `::1`.
  pub allow_loopback_network_access: bool,

  /// Allow HTTP tools to reach link-local addresses such as `169.254.0.0/16`.
  pub allow_link_local_network_access: bool,

  /// Allow HTTP tools to reach private RFC1918/ULA addresses.
  pub allow_private_network_access: bool,

  /// Allow HTTP tools to reach well-known cloud metadata endpoints.
  pub allow_cloud_metadata_access: bool,

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
      allow_loopback_network_access: false,
      allow_link_local_network_access: false,
      allow_private_network_access: false,
      allow_cloud_metadata_access: false,
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
      allow_loopback_network_access: true,
      allow_link_local_network_access: true,
      allow_private_network_access: true,
      allow_cloud_metadata_access: true,
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
    self.path_denial_reason(path).is_none()
  }

  /// Return an explanatory denial reason for a filesystem path.
  pub fn path_denial_reason(&self, path: &Path) -> Option<String> {
    if self.allowed_paths.is_empty() {
      return None; // permissive
    }

    if path
      .components()
      .any(|component| matches!(component, Component::ParentDir | Component::Prefix(_)))
    {
      return Some(format!(
        "path '{}' contains traversal or platform prefix components",
        path.display()
      ));
    }

    let comparable_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    if self
      .allowed_paths
      .iter()
      .any(|allowed| path_starts_with_allowed(&comparable_path, allowed))
    {
      None
    } else {
      Some(format!(
        "path '{}' is outside allowed path prefixes: {}",
        comparable_path.display(),
        self
          .allowed_paths
          .iter()
          .map(|path| path.display().to_string())
          .collect::<Vec<_>>()
          .join(", ")
      ))
    }
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

  /// Check whether a protected network address class is explicitly allowed.
  pub fn is_network_address_class_allowed(&self, class: NetworkAddressClass) -> bool {
    match class {
      NetworkAddressClass::Loopback => self.allow_loopback_network_access,
      NetworkAddressClass::LinkLocal => self.allow_link_local_network_access,
      NetworkAddressClass::Private => self.allow_private_network_access,
      NetworkAddressClass::CloudMetadata => self.allow_cloud_metadata_access,
    }
  }
}

fn path_starts_with_allowed(path: &Path, allowed: &Path) -> bool {
  let comparable_allowed = allowed
    .canonicalize()
    .unwrap_or_else(|_| allowed.to_path_buf());
  path.starts_with(comparable_allowed)
}
