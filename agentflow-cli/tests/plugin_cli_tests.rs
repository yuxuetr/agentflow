//! End-to-end CLI tests for `agentflow plugin {install,list,inspect,uninstall}`.
//!
//! Gated on `feature = "plugin"` so the default `cargo test -p agentflow-cli`
//! run stays free of the plugin runtime. The matrix job in
//! `.github/workflows/quality.yml` (cli-plugin) drives this file.
//!
//! Each test composes a self-contained "source" directory with a
//! `plugin.toml` and a stub entrypoint, runs the CLI against a temp
//! plugins root (via `--dir`), and asserts on stdout / filesystem
//! state. We never spawn the plugin subprocess in these tests — the
//! CLI commands are filesystem-level only, and the existing
//! `plugin_node_tests` module in `workflow_tests.rs` already covers
//! the `workflow run` path that does spawn.

#![cfg(feature = "plugin")]

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn write_source_plugin(dir: &Path, name: &str) -> PathBuf {
  let plugin_root = dir.join(format!("{name}-source"));
  fs::create_dir_all(plugin_root.join("bin")).unwrap();

  let manifest = format!(
    r#"
[plugin]
name = "{name}"
version = "0.1.0"
runtime = "subprocess"
entrypoint = "bin/{name}-entry"

[[plugin.nodes]]
type = "{name}_echo"
description = "Echo input back."

[plugin.capabilities]
filesystem = ["read:./data"]
network = []
processes = []
env_vars = ["DEMO_VAR"]
"#
  );
  fs::write(plugin_root.join("plugin.toml"), manifest).unwrap();

  let entrypoint = plugin_root.join("bin").join(format!("{name}-entry"));
  fs::write(&entrypoint, b"#!/bin/sh\nexit 0\n").unwrap();
  #[cfg(unix)]
  {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&entrypoint, fs::Permissions::from_mode(0o755)).unwrap();
  }

  plugin_root
}

#[test]
fn plugin_install_list_inspect_uninstall_round_trip() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let source = write_source_plugin(work.path(), "demo-plugin");

  // install
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Installed plugin: demo-plugin"));

  let installed = plugins_dir.join("demo-plugin");
  assert!(installed.join("plugin.toml").is_file());
  assert!(installed.join("bin").join("demo-plugin-entry").is_file());

  // list
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "list", "--dir", plugins_dir.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("demo-plugin v0.1.0"))
    .stdout(predicate::str::contains("demo-plugin_echo"))
    .stdout(predicate::str::contains("[ok]"));

  // inspect by directory
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "inspect", installed.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Plugin: demo-plugin"))
    .stdout(predicate::str::contains("Status: valid"))
    .stdout(predicate::str::contains("env_vars: DEMO_VAR"));

  // inspect by manifest path
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "inspect",
      installed.join("plugin.toml").to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Plugin: demo-plugin"));

  // uninstall
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "demo-plugin",
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("Uninstalled plugin 'demo-plugin'"));

  assert!(!installed.exists(), "plugin dir should be removed");
}

#[test]
fn plugin_install_rejects_source_without_manifest() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let bad_source = work.path().join("not-a-plugin");
  fs::create_dir_all(&bad_source).unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      bad_source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("does not contain a plugin.toml"));
}

#[test]
fn plugin_install_requires_force_to_overwrite() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let source = write_source_plugin(work.path(), "force-plugin");

  // First install succeeds.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .success();

  // Second install without --force fails with a clear message.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("--force to overwrite"));

  // With --force the second install succeeds.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
      "--force",
    ])
    .assert()
    .success();
}

#[test]
fn plugin_uninstall_unknown_plugin_fails_without_force() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  fs::create_dir_all(&plugins_dir).unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "nope",
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("'nope' is not installed"));

  // --force makes uninstall idempotent.
  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "nope",
      "--dir",
      plugins_dir.to_str().unwrap(),
      "--force",
    ])
    .assert()
    .success()
    .stdout(predicate::str::contains("nothing to do"));
}

#[test]
fn plugin_uninstall_refuses_directory_without_manifest() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let bogus = plugins_dir.join("bogus");
  fs::create_dir_all(&bogus).unwrap();
  fs::write(bogus.join("README.md"), "not a plugin").unwrap();

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "uninstall",
      "bogus",
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .assert()
    .failure()
    .stderr(predicate::str::contains("missing plugin.toml"));

  assert!(bogus.exists(), "bogus directory should not be removed");
}

#[test]
fn plugin_install_production_profile_rejects_unsigned_plugin() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let source = write_source_plugin(work.path(), "prod-plugin");

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
    ])
    .env("AGENTFLOW_SECURITY_PROFILE", "production")
    .assert()
    .failure()
    .stderr(
      predicate::str::contains("production")
        .and(predicate::str::contains("signature").or(predicate::str::contains("sandbox"))),
    );
  // The install must have been refused before any filesystem write.
  assert!(!plugins_dir.join("prod-plugin").exists());
}

#[test]
fn plugin_install_production_profile_refuses_allow_unsandboxed_opt_in() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins");
  let source = write_source_plugin(work.path(), "prod-opt-in");

  Command::cargo_bin("agentflow")
    .unwrap()
    .args([
      "plugin",
      "install",
      source.to_str().unwrap(),
      "--dir",
      plugins_dir.to_str().unwrap(),
      "--allow-unsandboxed-plugin",
      "--signed",
    ])
    .env("AGENTFLOW_SECURITY_PROFILE", "production")
    .assert()
    .failure()
    .stderr(predicate::str::contains(
      "refuses --allow-unsandboxed-plugin",
    ));
}

#[test]
fn plugin_install_help_lists_p18_flags() {
  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "install", "--help"])
    .assert()
    .success()
    .stdout(predicate::str::contains("--allow-unsandboxed-plugin"))
    .stdout(predicate::str::contains("--signed"));
}

#[test]
fn plugin_list_on_empty_dir_is_friendly() {
  let work = TempDir::new().unwrap();
  let plugins_dir = work.path().join("plugins-missing");

  Command::cargo_bin("agentflow")
    .unwrap()
    .args(["plugin", "list", "--dir", plugins_dir.to_str().unwrap()])
    .assert()
    .success()
    .stdout(predicate::str::contains("Plugins directory not found"));
}
