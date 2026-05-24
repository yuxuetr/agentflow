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
///
/// ## Allow-list semantics (Q1.2.1)
///
/// `allowed_commands` and `allowed_paths` now share a uniform contract:
/// an **empty list means deny everything**. To opt into permissive mode
/// (allow any command / any path) set the matching `allow_all_*` flag
/// explicitly. This matches the operator expectation that "no allow-list
/// configured" = "no access granted" — the prior asymmetric default (empty
/// `allowed_paths` falling open) silently exposed `FileTool` to write any
/// path on the host whenever the policy was constructed via
/// [`SandboxPolicy::default`].
#[derive(Debug, Clone)]
pub struct SandboxPolicy {
  /// Allowed shell commands (first token of the command string).
  /// If **empty**, ALL commands are blocked unless [`allow_all_commands`]
  /// is also set.
  ///
  /// [`allow_all_commands`]: SandboxPolicy::allow_all_commands
  pub allowed_commands: Vec<String>,

  /// Bypass for [`allowed_commands`]. When `true`, every command is
  /// allowed regardless of the list. Used by [`SandboxPolicy::permissive`].
  ///
  /// [`allowed_commands`]: SandboxPolicy::allowed_commands
  pub allow_all_commands: bool,

  /// Allowed path prefixes for file read/write/list operations.
  /// If **empty**, ALL paths are denied unless [`allow_all_paths`] is set.
  ///
  /// [`allow_all_paths`]: SandboxPolicy::allow_all_paths
  pub allowed_paths: Vec<PathBuf>,

  /// Bypass for [`allowed_paths`]. When `true`, every path is allowed
  /// regardless of the list. Used by [`SandboxPolicy::permissive`].
  ///
  /// [`allowed_paths`]: SandboxPolicy::allowed_paths
  pub allow_all_paths: bool,

  /// Allowed host suffixes for HTTP requests (e.g. `"example.com"`).
  /// If **empty**, ALL domains are allowed (permissive mode — left
  /// asymmetric vs. paths/commands because changing it would require
  /// the operator to enumerate every cloud endpoint they use).
  pub allowed_domains: Vec<String>,

  /// Allow HTTP tools to reach loopback addresses such as `127.0.0.1` and `::1`.
  pub allow_loopback_network_access: bool,

  /// Allow HTTP tools to reach link-local addresses such as `169.254.0.0/16`.
  pub allow_link_local_network_access: bool,

  /// Allow HTTP tools to reach private RFC1918/ULA addresses.
  pub allow_private_network_access: bool,

  /// Allow HTTP tools to reach well-known cloud metadata endpoints.
  pub allow_cloud_metadata_access: bool,

  /// Allow file tools to read or overwrite hardlinked regular files.
  pub allow_hardlinked_files: bool,

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
      allow_all_commands: false,
      allowed_paths: vec![],
      allow_all_paths: false,
      allowed_domains: vec![],
      allow_loopback_network_access: false,
      allow_link_local_network_access: false,
      allow_private_network_access: false,
      allow_cloud_metadata_access: false,
      allow_hardlinked_files: false,
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
      allowed_commands: vec![],
      allow_all_commands: true,
      allowed_paths: vec![],
      allow_all_paths: true,
      allowed_domains: vec![],
      allow_loopback_network_access: true,
      allow_link_local_network_access: true,
      allow_private_network_access: true,
      allow_cloud_metadata_access: true,
      allow_hardlinked_files: true,
      max_exec_time_secs: 60,
      max_file_read_bytes: 100 * 1024 * 1024,
    }
  }

  /// Check whether a shell command is allowed.
  /// `cmd` should be the first whitespace-separated token.
  pub fn is_command_allowed(&self, cmd: &str) -> bool {
    if self.allow_all_commands {
      return true;
    }
    self.allowed_commands.iter().any(|c| c == cmd)
  }

  /// Check whether a filesystem path is reachable under the policy.
  pub fn is_path_allowed(&self, path: &Path) -> bool {
    self.path_denial_reason(path).is_none()
  }

  /// Return an explanatory denial reason for a filesystem path.
  pub fn path_denial_reason(&self, path: &Path) -> Option<String> {
    if self.allow_all_paths {
      return None;
    }
    if self.allowed_paths.is_empty() {
      return Some(format!(
        "path '{}' denied: no paths are allowed by this sandbox policy (set allow_all_paths or populate allowed_paths)",
        path.display()
      ));
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

    let comparable_path = canonicalize_existing_prefix(path);
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

fn canonicalize_existing_prefix(path: &Path) -> PathBuf {
  if let Ok(canonical) = path.canonicalize() {
    return canonical;
  }

  let mut existing = path.to_path_buf();
  let mut missing = Vec::new();
  while !existing.as_os_str().is_empty() && !existing.exists() {
    if let Some(name) = existing.file_name() {
      missing.push(name.to_os_string());
    }
    if !existing.pop() {
      break;
    }
  }

  let mut comparable = existing.canonicalize().unwrap_or(existing);
  for component in missing.iter().rev() {
    comparable.push(component);
  }
  comparable
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Q1.2.1: the regression we're paid to prevent — `default()` must not
  /// accidentally allow `/etc/passwd` writes because nobody populated
  /// `allowed_paths`.
  #[test]
  fn default_policy_denies_arbitrary_paths() {
    let policy = SandboxPolicy::default();
    assert!(!policy.allow_all_paths);
    assert!(policy.allowed_paths.is_empty());

    let reason = policy.path_denial_reason(Path::new("/etc/passwd"));
    assert!(
      reason.is_some(),
      "default policy must deny /etc/passwd but allowed it"
    );
    let message = reason.unwrap();
    assert!(
      message.contains("no paths are allowed"),
      "denial message should explain the empty allow-list: {message}"
    );
  }

  /// Default policy must also deny arbitrary commands when none are
  /// listed (the analogous guarantee for `allowed_commands`).
  #[test]
  fn default_policy_only_allows_curated_command_set() {
    let policy = SandboxPolicy::default();
    assert!(!policy.allow_all_commands);
    assert!(policy.is_command_allowed("echo"));
    assert!(!policy.is_command_allowed("rm"));
  }

  /// `permissive()` must explicitly set both `allow_all_*` bypass bits
  /// so it actually behaves permissively after the empty-list flip.
  #[test]
  fn permissive_policy_sets_explicit_allow_all_bits() {
    let policy = SandboxPolicy::permissive();
    assert!(policy.allow_all_commands);
    assert!(policy.allow_all_paths);

    assert!(policy.is_command_allowed("anything"));
    assert!(policy.path_denial_reason(Path::new("/anywhere")).is_none());
  }

  #[test]
  fn explicit_allowed_paths_still_filter_after_flip() {
    // Use a real temp dir so the path canonicalization in
    // `canonicalize_existing_prefix` agrees with `path_starts_with_allowed`'s
    // canonicalization of the allow-list entry (on macOS `/tmp` is a
    // symlink to `/private/tmp`, so synthetic paths would mismatch).
    let temp = tempfile::TempDir::new().expect("create temp dir");
    let allowed_root = temp.path().to_path_buf();
    let policy = SandboxPolicy {
      allowed_paths: vec![allowed_root.clone()],
      ..SandboxPolicy::default()
    };

    let inside = allowed_root.join("foo");
    assert!(
      policy.path_denial_reason(&inside).is_none(),
      "path inside allow-list must not be denied"
    );

    let outside = temp.path().parent().unwrap().join("agentflow_q121_outside");
    assert!(
      policy.path_denial_reason(&outside).is_some(),
      "path outside allow-list must be denied"
    );
  }
}
