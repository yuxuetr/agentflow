//! Workspace automation entry point.
//!
//! Run with `cargo xtask <subcommand>` (alias defined in `.cargo/config.toml`).
//! Subcommands available today:
//!
//! - `verify-edition` — assert every workspace member declares
//!   `edition = "2024"` so a freshly-added crate cannot silently drift to a
//!   different edition (`M.6`).
//! - `check-agent-sdk-doc` — scan `docs/AGENT_SDK.md` for backtick-quoted
//!   `CamelCase` identifiers and assert each one has a matching definition
//!   (`pub trait|struct|enum|type|fn`) somewhere in the workspace `src/`
//!   tree. Catches doc rot when traits / types referenced in the SDK guide
//!   are renamed or removed without updating the doc (`M.2`).

use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

const EXPECTED_EDITION: &str = "2024";

const AGENT_SDK_DOC: &str = "docs/AGENT_SDK.md";

/// Backtick-quoted identifiers in `AGENT_SDK.md` that aren't workspace types:
/// stdlib / enum variants / pluralised names / one-shot example types. Adding
/// to this set is the escape hatch when the grep heuristic produces a false
/// positive — keep it small and document why each entry is here.
const AGENT_SDK_ALLOWLIST: &[&str] = &[
  // Standard library / language primitives.
  "Err",
  "None",
  // Enum variants of types that *do* exist in the codebase (the variant name
  // doesn't have its own `pub` declaration but the parent type is covered).
  "Step",
  "Plan",
  "Reflect",
  "Failure",
  "Critique",
  "Final",
  "FailureReason",
  // Example types defined inline in the doc (no real impl file).
  "EchoTool",
];

fn main() -> Result<()> {
  let mut args = std::env::args().skip(1);
  let subcommand = args.next().unwrap_or_default();
  match subcommand.as_str() {
    "verify-edition" => {
      let workspace_root = workspace_root();
      verify_edition_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-agent-sdk-doc" => {
      let workspace_root = workspace_root();
      check_agent_sdk_doc_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    other => {
      print_usage(&mut std::io::stderr());
      if other.is_empty() {
        bail!("missing subcommand");
      }
      bail!("unknown subcommand '{other}'");
    }
  }
}

fn print_usage(sink: &mut impl Write) {
  let _ = writeln!(sink, "usage: cargo xtask <subcommand>");
  let _ = writeln!(sink, "subcommands:");
  let _ = writeln!(
    sink,
    "  verify-edition       fail if any workspace member declares an edition other than \"{EXPECTED_EDITION}\""
  );
  let _ = writeln!(
    sink,
    "  check-agent-sdk-doc  fail if {AGENT_SDK_DOC} references a CamelCase type that does not exist under any agentflow-*/src/**/*.rs"
  );
}

/// Run the edition-pin check against `workspace_root` and report through the
/// caller-supplied sinks. Returns `Ok(())` on a clean workspace and a context-
/// rich error when one or more members declare an unexpected edition.
fn verify_edition_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let members = read_workspace_members(workspace_root)?;
  let mut failures: Vec<String> = Vec::new();
  let mut checked: Vec<String> = Vec::new();
  for member in &members {
    let manifest = workspace_root.join(member).join("Cargo.toml");
    let edition = read_edition(&manifest)
      .with_context(|| format!("Failed to read edition for member '{}'", manifest.display()))?;
    if edition != EXPECTED_EDITION {
      failures.push(format!(
        "  - {}: edition = \"{}\" (expected \"{}\")",
        member, edition, EXPECTED_EDITION
      ));
    }
    checked.push(member.clone());
  }
  writeln!(
    stdout,
    "verify-edition: checked {} workspace member(s) against edition \"{}\"",
    checked.len(),
    EXPECTED_EDITION
  )?;
  if failures.is_empty() {
    writeln!(stdout, "verify-edition: OK")?;
    return Ok(());
  }
  writeln!(stderr, "verify-edition: FAIL")?;
  for line in &failures {
    writeln!(stderr, "{line}")?;
  }
  bail!(
    "{} workspace member(s) declare an unexpected edition",
    failures.len()
  );
}

/// Run the agent-SDK doc drift check against `workspace_root`. Returns
/// `Ok(())` when every CamelCase identifier the doc cites has either a real
/// `pub` definition under any `agentflow-*/src/**/*.rs` or is on the
/// allowlist.
fn check_agent_sdk_doc_at(
  workspace_root: &Path,
  stdout: &mut impl Write,
  stderr: &mut impl Write,
) -> Result<()> {
  let doc_path = workspace_root.join(AGENT_SDK_DOC);
  let doc = std::fs::read_to_string(&doc_path)
    .with_context(|| format!("Failed to read {}", doc_path.display()))?;
  let mentions = extract_camelcase_mentions(&doc);
  let known_definitions = collect_workspace_pub_definitions(workspace_root)?;
  let allowlist: BTreeSet<&str> = AGENT_SDK_ALLOWLIST.iter().copied().collect();
  let mut missing: Vec<String> = Vec::new();
  let mut checked: usize = 0;
  for name in &mentions {
    if allowlist.contains(name.as_str()) {
      continue;
    }
    checked += 1;
    if !known_definitions.contains(name.as_str()) {
      missing.push(name.clone());
    }
  }
  writeln!(
    stdout,
    "check-agent-sdk-doc: cross-referenced {} CamelCase mention(s) in {} ({} ignored via allowlist)",
    checked,
    AGENT_SDK_DOC,
    mentions.len() - checked
  )?;
  if missing.is_empty() {
    writeln!(stdout, "check-agent-sdk-doc: OK")?;
    return Ok(());
  }
  writeln!(stderr, "check-agent-sdk-doc: FAIL")?;
  for name in &missing {
    writeln!(
      stderr,
      "  - `{name}`: referenced in {AGENT_SDK_DOC} but no `pub (trait|struct|enum|type|fn) {name}` declaration found in any workspace src/ tree"
    )?;
  }
  bail!(
    "{} identifier(s) in {} have no matching workspace declaration",
    missing.len(),
    AGENT_SDK_DOC
  );
}

/// Pull every `` `CamelCaseIdent` `` (non-empty, starts with uppercase letter,
/// only alphanumerics after) out of the doc. Returns a sorted dedup list so
/// CI output diff is stable.
fn extract_camelcase_mentions(doc: &str) -> Vec<String> {
  let mut hits: BTreeSet<String> = BTreeSet::new();
  let bytes = doc.as_bytes();
  let mut i = 0;
  while i < bytes.len() {
    if bytes[i] != b'`' {
      i += 1;
      continue;
    }
    let start = i + 1;
    let mut end = start;
    while end < bytes.len() && bytes[end] != b'`' && bytes[end] != b'\n' {
      end += 1;
    }
    if end < bytes.len() && bytes[end] == b'`' {
      let token = &doc[start..end];
      if is_camelcase_ident(token) {
        hits.insert(token.to_string());
      }
      i = end + 1;
    } else {
      i = end + 1;
    }
  }
  hits.into_iter().collect()
}

fn is_camelcase_ident(s: &str) -> bool {
  let mut chars = s.chars();
  let first = match chars.next() {
    Some(c) => c,
    None => return false,
  };
  if !first.is_ascii_uppercase() {
    return false;
  }
  for c in chars {
    if !c.is_ascii_alphanumeric() {
      return false;
    }
  }
  // Require either a lowercase letter or another uppercase letter after the
  // first character — pure-uppercase tokens like `JSON` or `URL` are usually
  // acronyms in prose, not workspace types.
  s.chars().skip(1).any(|c| c.is_ascii_lowercase())
}

/// Collect every `pub (trait|struct|enum|type|fn) Ident` name declared
/// anywhere under `<workspace_root>/agentflow-*/src/**/*.rs`. Matches both
/// bare `pub` and visibility-restricted (`pub(crate)`, `pub(super)`, …)
/// forms so internal-but-discoverable types still count toward the doc
/// cross-reference.
fn collect_workspace_pub_definitions(workspace_root: &Path) -> Result<BTreeSet<String>> {
  let mut out: BTreeSet<String> = BTreeSet::new();
  for member in read_workspace_members(workspace_root)? {
    let src = workspace_root.join(&member).join("src");
    if !src.exists() {
      continue;
    }
    walk_rs(&src, &mut |path| {
      let content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;
      for ident in scan_pub_idents(&content) {
        out.insert(ident);
      }
      Ok(())
    })?;
  }
  Ok(out)
}

fn walk_rs(root: &Path, visit: &mut impl FnMut(&Path) -> Result<()>) -> Result<()> {
  for entry in std::fs::read_dir(root)
    .with_context(|| format!("Failed to read directory {}", root.display()))?
  {
    let entry = entry?;
    let path = entry.path();
    let ty = entry.file_type()?;
    if ty.is_dir() {
      walk_rs(&path, visit)?;
    } else if ty.is_file()
      && path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("rs"))
    {
      visit(&path)?;
    }
  }
  Ok(())
}

/// Scan a single `.rs` file for `pub …` declarations and yield the declared
/// identifier names. Handles both naked `pub` and visibility-restricted
/// (`pub(crate)`, `pub(super)`, `pub(in path)`) forms. The matcher is
/// intentionally simple — false negatives are tolerated; false positives are
/// not (they would mask real drift), so the keyword set is short and exact.
fn scan_pub_idents(content: &str) -> Vec<String> {
  let mut out: Vec<String> = Vec::new();
  for line in content.lines() {
    let trimmed = line.trim_start();
    // Skip the "pub(...)" parenthesis prefix if present so the kind keyword
    // comparison below is the same for `pub fn` and `pub(crate) fn`.
    let after_pub = if let Some(rest) = trimmed.strip_prefix("pub") {
      let rest = rest.trim_start();
      if rest.starts_with('(') {
        match rest.find(')') {
          Some(end) => rest[end + 1..].trim_start(),
          None => continue,
        }
      } else {
        rest
      }
    } else {
      continue;
    };
    let (kind, body) = match after_pub.split_once(char::is_whitespace) {
      Some(pair) => pair,
      None => continue,
    };
    let kind = kind.trim();
    if !matches!(kind, "trait" | "struct" | "enum" | "type" | "fn") {
      continue;
    }
    // Strip optional `unsafe`, `async`, `default` modifiers and grab the
    // identifier as the leading [A-Za-z0-9_]+ token.
    let body = body.trim_start();
    let ident: String = body
      .chars()
      .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
      .collect();
    if !ident.is_empty() {
      out.push(ident);
    }
  }
  out
}

fn workspace_root() -> PathBuf {
  // `CARGO_MANIFEST_DIR` for the xtask crate is `<workspace>/xtask`.
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  manifest_dir
    .parent()
    .map(PathBuf::from)
    .unwrap_or(manifest_dir)
}

fn read_workspace_members(workspace_root: &Path) -> Result<Vec<String>> {
  let manifest_path = workspace_root.join("Cargo.toml");
  let content = std::fs::read_to_string(&manifest_path)
    .with_context(|| format!("Failed to read {}", manifest_path.display()))?;
  let parsed: toml::Value = toml::from_str(&content)
    .with_context(|| format!("Failed to parse {}", manifest_path.display()))?;
  let members = parsed
    .get("workspace")
    .and_then(|w| w.get("members"))
    .and_then(|m| m.as_array())
    .ok_or_else(|| anyhow::anyhow!("workspace.members array missing in root Cargo.toml"))?;
  let mut out: Vec<String> = Vec::with_capacity(members.len());
  for entry in members {
    if let Some(name) = entry.as_str() {
      // Skip xtask itself: it's part of the workspace but its own edition is
      // governed by the same rule, so include it. Only deliberate skip: none.
      out.push(name.to_string());
    }
  }
  // Stable iteration order so CI logs diff cleanly.
  out.sort();
  Ok(out)
}

fn read_edition(manifest: &Path) -> Result<String> {
  let content = std::fs::read_to_string(manifest)
    .with_context(|| format!("Failed to read {}", manifest.display()))?;
  let parsed: toml::Value =
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", manifest.display()))?;
  let edition = parsed
    .get("package")
    .and_then(|p| p.get("edition"))
    .and_then(|e| e.as_str())
    .ok_or_else(|| {
      anyhow::anyhow!(
        "package.edition missing from {} — every workspace member must declare an edition",
        manifest.display()
      )
    })?;
  Ok(edition.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Write a synthetic workspace under `root` with the given members; each
  /// (`name`, `edition`) tuple becomes a `<root>/<name>/Cargo.toml` with the
  /// requested edition.
  fn write_synthetic_workspace(root: &Path, members: &[(&str, &str)]) {
    let members_lines: String = members
      .iter()
      .map(|(name, _)| format!("  \"{name}\",\n"))
      .collect();
    let root_manifest = format!("[workspace]\nmembers = [\n{members_lines}]\nresolver = \"2\"\n");
    std::fs::write(root.join("Cargo.toml"), root_manifest).unwrap();
    for (name, edition) in members {
      let member_dir = root.join(name);
      std::fs::create_dir_all(&member_dir).unwrap();
      let manifest =
        format!("[package]\nname = \"{name}\"\nversion = \"0.0.0\"\nedition = \"{edition}\"\n");
      std::fs::write(member_dir.join("Cargo.toml"), manifest).unwrap();
    }
  }

  fn tempdir() -> tempfile::TempDir {
    tempfile::tempdir().expect("create tempdir")
  }

  #[test]
  fn passes_when_every_member_is_pinned() {
    let root = tempdir();
    write_synthetic_workspace(
      root.path(),
      &[("alpha", EXPECTED_EDITION), ("beta", EXPECTED_EDITION)],
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("checked 2 workspace member(s)"));
    assert!(stdout_s.contains("verify-edition: OK"));
  }

  #[test]
  fn fails_when_member_uses_wrong_edition() {
    let root = tempdir();
    write_synthetic_workspace(
      root.path(),
      &[("alpha", EXPECTED_EDITION), ("legacy", "2021")],
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected wrong-edition failure");
    let err_msg = format!("{err:#}");
    assert!(err_msg.contains("1 workspace member"), "error: {err_msg}");
    let stderr_s = String::from_utf8(stderr).unwrap();
    assert!(
      stderr_s.contains("legacy: edition = \"2021\""),
      "stderr: {stderr_s}"
    );
    assert!(stderr_s.contains("verify-edition: FAIL"));
  }

  #[test]
  fn errors_when_member_omits_edition_entirely() {
    let root = tempdir();
    // Skip the helper because it always writes an edition; craft by hand.
    let root_manifest = "[workspace]\nmembers = [\"orphan\"]\nresolver = \"2\"\n";
    std::fs::write(root.path().join("Cargo.toml"), root_manifest).unwrap();
    std::fs::create_dir_all(root.path().join("orphan")).unwrap();
    std::fs::write(
      root.path().join("orphan/Cargo.toml"),
      "[package]\nname = \"orphan\"\nversion = \"0.0.0\"\n",
    )
    .unwrap();
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = verify_edition_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected missing-edition failure");
    let err_msg = format!("{err:#}");
    assert!(
      err_msg.contains("package.edition missing"),
      "error: {err_msg}"
    );
  }

  // ── check-agent-sdk-doc ──────────────────────────────────────────────

  /// Build a synthetic workspace under `root` with a single member crate
  /// `mock-crate` containing a `src/lib.rs` with the supplied declarations,
  /// plus a `docs/AGENT_SDK.md` file with the supplied body.
  fn write_synthetic_doc_workspace(root: &Path, src_lib: &str, agent_sdk_doc: &str) {
    let root_manifest = "[workspace]\nmembers = [\"mock-crate\"]\nresolver = \"2\"\n";
    std::fs::write(root.join("Cargo.toml"), root_manifest).unwrap();
    let crate_dir = root.join("mock-crate");
    let src_dir = crate_dir.join("src");
    std::fs::create_dir_all(&src_dir).unwrap();
    std::fs::write(
      crate_dir.join("Cargo.toml"),
      "[package]\nname = \"mock-crate\"\nversion = \"0.0.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    std::fs::write(src_dir.join("lib.rs"), src_lib).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join(AGENT_SDK_DOC), agent_sdk_doc).unwrap();
  }

  #[test]
  fn agent_sdk_doc_check_passes_when_every_mention_has_a_pub_declaration() {
    let root = tempdir();
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\npub struct MyHandle;\npub enum MyKind { A, B }\n",
      "# SDK\nThe `MyRuntime` trait wraps a `MyHandle` and emits `MyKind` events.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("check-agent-sdk-doc: OK"));
    assert!(stdout_s.contains("3 CamelCase mention"));
  }

  #[test]
  fn agent_sdk_doc_check_fails_when_doc_references_renamed_type() {
    let root = tempdir();
    // Doc references both `RenamedRuntime` (gone) and `MyRuntime` (still
    // there). Expect the failure to name only the missing one.
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\n",
      "# SDK\nDescribed by `MyRuntime` and the older `RenamedRuntime`.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    let err = result.expect_err("expected missing-type failure");
    let err_msg = format!("{err:#}");
    assert!(err_msg.contains("1 identifier"), "error: {err_msg}");
    let stderr_s = String::from_utf8(stderr).unwrap();
    assert!(
      stderr_s.contains("`RenamedRuntime`"),
      "stderr should name the missing type; got:\n{stderr_s}"
    );
    assert!(
      !stderr_s.contains("`MyRuntime`"),
      "stderr should NOT name the still-present type; got:\n{stderr_s}"
    );
  }

  #[test]
  fn agent_sdk_doc_check_honors_allowlist() {
    // `None` and `Err` are on the allowlist; they should never trigger a
    // failure even when they appear in the doc with no matching pub decl.
    let root = tempdir();
    write_synthetic_doc_workspace(
      root.path(),
      "pub trait MyRuntime {}\n",
      "# SDK\nThe `MyRuntime` returns `None` on miss or `Err` on failure.\n",
    );
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let result = check_agent_sdk_doc_at(root.path(), &mut stdout, &mut stderr);
    assert!(result.is_ok(), "{}", String::from_utf8_lossy(&stderr));
    let stdout_s = String::from_utf8(stdout).unwrap();
    assert!(stdout_s.contains("ignored via allowlist"));
  }

  #[test]
  fn camelcase_extractor_skips_lowercase_inline_code_and_acronyms() {
    let doc = "Mix of `myFunc`, `JSON`, `MyType`, `URL`, and `AnotherType`.\n";
    let mentions = extract_camelcase_mentions(doc);
    assert_eq!(
      mentions,
      vec!["AnotherType".to_string(), "MyType".to_string()]
    );
  }

  #[test]
  fn pub_ident_scanner_handles_visibility_restricted_forms() {
    let src = "pub trait A {}\npub(crate) struct B;\npub(super) enum C { X }\nfn private() {}\n";
    let idents = scan_pub_idents(src);
    assert!(idents.contains(&"A".to_string()));
    assert!(idents.contains(&"B".to_string()));
    assert!(idents.contains(&"C".to_string()));
    assert!(!idents.contains(&"private".to_string()));
  }
}
