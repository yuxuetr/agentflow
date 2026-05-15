//! Smoke tests for `agentflow cleanup`. Actually running the sweep
//! requires Postgres; that path is exercised by
//! `agentflow-server/tests/cleanup_route.rs` when
//! `AGENTFLOW_DATABASE_TEST_URL` is set. Here we just check the
//! subcommand wiring and help surface.

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn cleanup_help_lists_documented_flags() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["cleanup", "--help"]);
  cmd
    .assert()
    .success()
    .stdout(predicate::str::contains("--database-url"))
    .stdout(predicate::str::contains("--run-dir"))
    .stdout(predicate::str::contains("--security-profile"))
    .stdout(predicate::str::contains("--dry-run"));
}

#[test]
fn cleanup_rejects_unknown_security_profile() {
  let mut cmd = Command::cargo_bin("agentflow").unwrap();
  cmd.args(["cleanup", "--security-profile", "bogus", "--dry-run"]);
  cmd.assert().failure();
}
