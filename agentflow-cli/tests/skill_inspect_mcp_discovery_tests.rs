//! Hermetic CLI coverage for `skill inspect` MCP discovery flag
//! behaviour (P10.9.1).
//!
//! These tests exercise the CLI surface — flag parsing, dispatch,
//! and the visible behaviour-toggling — without spawning any real
//! MCP servers. Tests that would require spawning use
//! `--no-mcp-discovery` to skip the spawn path; the cache module's
//! own unit tests (in src/commands/skill/mcp_discovery_cache.rs)
//! cover the hash + load + save round-trips.

use assert_cmd::Command;

fn cli_bin() -> Command {
  Command::cargo_bin("agentflow").expect("agentflow binary built")
}

/// Path to the bundled `rust_expert` skill (no MCP servers
/// declared). Used as the no-MCP baseline.
const RUST_EXPERT_DIR: &str = "../agentflow-skills/examples/skills/rust_expert";

/// Build a minimal SKILL.md with one MCP server declared. The
/// `command` is `python3` (on the default `mcp_command_allowlist`
/// so the validator accepts it) but `args` points to a file path
/// that doesn't exist — so any real spawn attempt would fail
/// loudly, while `--no-mcp-discovery` succeeds because the spawn
/// is short-circuited. The "would fail without the flag" property
/// is what makes this a meaningful proof.
fn write_mcp_skill(dir: &std::path::Path) {
  let skill_md = r#"---
name: p10-9-1-test
description: Hermetic test skill for the P10.9.1 inspect flags.
metadata:
  version: "0.1.0"
mcp_servers:
  - name: unreachable
    command: python3
    args:
      - /nonexistent/script-that-will-never-spawn.py
---

# Test Skill

This skill exists only to exercise the `agentflow skill inspect`
flag behaviour around MCP discovery. The declared MCP server's
script is intentionally invalid — any real spawn attempt fails
loudly. With `--no-mcp-discovery` the spawn is short-circuited.
"#;
  std::fs::write(dir.join("SKILL.md"), skill_md).expect("write SKILL.md");
}

#[test]
fn cli_skill_inspect_with_mcp_discovery_flag_emits_deprecation_warning() {
  // Skill has no MCP servers, so the discovery short-circuits
  // regardless. The deprecation warning is what we're asserting on:
  // operators with --with-mcp-discovery in their scripts must see
  // a one-line stderr note that the flag is now a no-op.
  let assert = cli_bin()
    .args([
      "skill",
      "inspect",
      RUST_EXPERT_DIR,
      "--explain-permissions",
      "--with-mcp-discovery",
    ])
    .assert()
    .success();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    stderr.contains("--with-mcp-discovery is now the default"),
    "deprecation warning missing from stderr: {stderr}"
  );
  // The warning must point operators at the new opt-out so they
  // know what to do if they actually wanted to skip discovery.
  assert!(
    stderr.contains("--no-mcp-discovery"),
    "warning must name the new opt-out flag: {stderr}"
  );
}

#[test]
fn cli_skill_inspect_without_mcp_discovery_flag_emits_no_warning() {
  // Baseline: a plain `skill inspect --explain-permissions` on a
  // skill with no MCP servers must NOT emit the deprecation
  // warning. Pinning the absence catches a regression that fires
  // the warning unconditionally.
  let assert = cli_bin()
    .args(["skill", "inspect", RUST_EXPERT_DIR, "--explain-permissions"])
    .assert()
    .success();
  let stderr = String::from_utf8(assert.get_output().stderr.clone()).unwrap();
  assert!(
    !stderr.contains("--with-mcp-discovery"),
    "no deprecation warning should fire when the flag isn't passed: {stderr}"
  );
}

#[test]
fn cli_skill_inspect_no_mcp_discovery_skips_spawn_with_message() {
  // The skill DOES declare an MCP server (with an unreachable
  // command). With `--no-mcp-discovery` the CLI must NOT attempt
  // to spawn — it should succeed and print the "skipped" notice.
  // Without `--no-mcp-discovery` the same skill would try to
  // spawn `/nonexistent/binary-that-will-never-spawn` and fail.
  // The success exit is itself the proof that the spawn was
  // short-circuited.
  let dir = tempfile::tempdir().expect("tempdir");
  write_mcp_skill(dir.path());

  let assert = cli_bin()
    .args([
      "skill",
      "inspect",
      dir.path().to_str().unwrap(),
      "--explain-permissions",
      "--no-mcp-discovery",
    ])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  assert!(
    stdout.contains("MCP discovery: skipped (--no-mcp-discovery)"),
    "expected the skipped notice in stdout: {stdout}"
  );
  // The notice must explain the downstream consequence (admission
  // rows will be incomplete) so operators understand the trade-off.
  assert!(
    stdout.contains("admission rows") || stdout.contains("will not include"),
    "skipped notice should explain the trade-off: {stdout}"
  );
}

#[test]
fn cli_skill_inspect_no_mcp_discovery_without_explain_permissions_warns() {
  // The flag only takes effect with `--explain-permissions`. The
  // CLI must surface a note when the operator passes it alone, so
  // a stray flag doesn't silently no-op.
  let assert = cli_bin()
    .args(["skill", "inspect", RUST_EXPERT_DIR, "--no-mcp-discovery"])
    .assert()
    .success();
  let stdout = String::from_utf8(assert.get_output().stdout.clone()).unwrap();
  assert!(
    stdout.contains("only honored with --explain-permissions"),
    "stray --no-mcp-discovery must surface a note: {stdout}"
  );
}
