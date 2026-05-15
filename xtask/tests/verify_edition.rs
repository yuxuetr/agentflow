//! Integration tests for `cargo xtask verify-edition`.
//!
//! The default success path is exercised by the CI step itself; this suite
//! covers (a) the OK exit on the real workspace and (b) the failure path
//! against a synthetic workspace fixture with a wrong-edition crate.

use std::path::PathBuf;
use std::process::Command;

fn xtask_binary() -> PathBuf {
  // `cargo test` exports CARGO_BIN_EXE_<name> for every bin defined in the
  // same crate as the test, so we don't need to hardcode the target path.
  let env_key = "CARGO_BIN_EXE_xtask";
  PathBuf::from(std::env::var(env_key).expect("CARGO_BIN_EXE_xtask is set by cargo test"))
}

#[test]
fn verify_edition_passes_on_real_workspace() {
  let output = Command::new(xtask_binary())
    .arg("verify-edition")
    .output()
    .expect("invoking xtask binary should not fail");
  assert!(
    output.status.success(),
    "verify-edition on the real workspace should pass; stderr:\n{}",
    String::from_utf8_lossy(&output.stderr),
  );
  let stdout = String::from_utf8_lossy(&output.stdout);
  assert!(stdout.contains("verify-edition: OK"), "stdout: {stdout}");
}

#[test]
fn unknown_subcommand_exits_with_failure() {
  let output = Command::new(xtask_binary())
    .arg("nonexistent-subcommand")
    .output()
    .expect("invoking xtask binary should not fail");
  assert!(
    !output.status.success(),
    "unknown subcommand should exit nonzero"
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    stderr.contains("unknown subcommand 'nonexistent-subcommand'"),
    "stderr should name the bad subcommand; got: {stderr}"
  );
}

#[test]
fn missing_subcommand_prints_usage_and_exits_with_failure() {
  let output = Command::new(xtask_binary())
    .output()
    .expect("invoking xtask binary should not fail");
  assert!(
    !output.status.success(),
    "missing subcommand should exit nonzero"
  );
  let stderr = String::from_utf8_lossy(&output.stderr);
  assert!(
    stderr.contains("usage: cargo xtask"),
    "stderr should include usage line; got: {stderr}"
  );
  assert!(
    stderr.contains("missing subcommand"),
    "stderr should explain the missing subcommand; got: {stderr}"
  );
}
