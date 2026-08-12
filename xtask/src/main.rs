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
//! - `examples-smoke` — compile and run each SDK example from the
//!   canonical matrix (`examples/README.md`) with a per-example wall-
//!   clock cap; fail the workspace if any example panics or exceeds the
//!   cap. Backs the P3.2 / P3.10 / P7.3 CI gate.
//! - `bench-gate` — compare the latest Criterion run under
//!   `target/criterion/` against a checked-in baseline JSON; exit
//!   non-zero when any benchmark's median wall-clock is at least the
//!   regression threshold above baseline. Backs the P7.2 perf gate.
//! - `check-changelog` — fail when a non-trivial source change versus
//!   the base ref (default `origin/main`) didn't touch `CHANGELOG.md`
//!   AND no commit body in the branch range carries the
//!   `chore(skip-changelog)` opt-out marker (P10.18.2).
//! - `test-gate` — run `cargo test -p <crate>` per workspace member,
//!   capture wall-clock per crate, compare against a checked-in
//!   baseline JSON, and fail when any crate's ratio crosses the
//!   regression threshold (default 1.5×). Pair to `bench-gate` for
//!   test-suite-bloat detection (P10.19.2).
//! - `refresh-live-models` — for each provider wired into the
//!   `llm-live` nightly workflow, ping the provider's `/models`
//!   endpoint and verify the hard-coded text-model default still
//!   exists. Reports per-provider status + suggests replacements
//!   when the default 404s (P10.3.4).
//! - `redaction-lint` — grep every `agentflow-*/src/**/*.rs` for
//!   `(debug|info|warn|error)!(... danger = %text, ...)` patterns
//!   that interpolate raw user prompt / response / content / body /
//!   params into a log macro without going through
//!   `agentflow_tracing::redaction` or `prompt_fingerprint`. Backs
//!   the Q5.2 workspace redaction audit.
//! - `check-arch` — assert the subset of the eight crate-dependency laws
//!   (`docs/RFC_CRATE_ARCHITECTURE.md` §7) checkable today: runtime-isolation,
//!   surface-isolation, and kernel-isolation (R1.2 — an L0 contract crate must
//!   not depend on anything outside the L0 kernel set). Known current
//!   violations live in `ARCH_ALLOWLIST` with a P-A burndown task; the gate
//!   fails on any NEW violation or any stale allowlist entry, so the list can
//!   only shrink (P-A0.2).
//!
//! Each subcommand's implementation lives in its own file under
//! `tasks/` (one file per subcommand); this file only wires up arg
//! parsing/dispatch, the shared `workspace_root()` / `read_workspace_members()`
//! helpers, and the `print_usage` text.

mod tasks;

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};

pub(crate) const EXPECTED_EDITION: &str = "2024";

pub(crate) const AGENT_SDK_DOC: &str = "docs/AGENT_SDK.md";

fn main() -> Result<()> {
  let mut args = std::env::args().skip(1);
  let subcommand = args.next().unwrap_or_default();
  match subcommand.as_str() {
    "verify-edition" => {
      let workspace_root = workspace_root();
      tasks::verify_edition::verify_edition_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-agent-sdk-doc" => {
      let workspace_root = workspace_root();
      tasks::check_agent_sdk_doc::check_agent_sdk_doc_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "examples-smoke" => {
      let workspace_root = workspace_root();
      tasks::examples_smoke::examples_smoke_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "bench-gate" => {
      let workspace_root = workspace_root();
      tasks::bench_gate::bench_gate_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-changelog" => {
      let workspace_root = workspace_root();
      tasks::check_changelog::check_changelog_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "test-gate" => {
      let workspace_root = workspace_root();
      tasks::test_gate::test_gate_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "refresh-live-models" => {
      let workspace_root = workspace_root();
      tasks::refresh_live_models::refresh_live_models_from_args(
        &workspace_root,
        args.collect::<Vec<_>>(),
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "redaction-lint" => {
      let workspace_root = workspace_root();
      tasks::redaction_lint::redaction_lint_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "check-arch" => {
      let workspace_root = workspace_root();
      tasks::check_arch::check_arch_at(
        &workspace_root,
        &mut std::io::stdout(),
        &mut std::io::stderr(),
      )
    }
    "println-lint" => {
      let workspace_root = workspace_root();
      tasks::println_lint::println_lint_at(
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
  let _ = writeln!(
    sink,
    "  examples-smoke       compile + run each SDK example from examples/README.md with a per-example wall-clock cap; fail on panic or timeout"
  );
  let _ = writeln!(
    sink,
    "  bench-gate           compare target/criterion/* against benches/baselines/<host>.json; fail when median ≥ 1.25× baseline"
  );
  let _ = writeln!(
    sink,
    "  check-changelog [BASE]  fail if a non-trivial source change vs BASE (default origin/main) didn't touch CHANGELOG.md AND no commit body carries `chore(skip-changelog)`"
  );
  let _ = writeln!(
    sink,
    "  test-gate            run `cargo test -p <crate>` per workspace member, compare wall-clock against benches/baselines/test-timings/<host>.json; fail when ratio ≥ 1.5×"
  );
  let _ = writeln!(
    sink,
    "  refresh-live-models  ping each provider's /models endpoint with the key from ~/.agentflow/.env (or env), report whether the live-test default still exists, suggest replacements on 404 (P10.3.4)"
  );
  let _ = writeln!(
    sink,
    "  redaction-lint       grep agentflow-*/src/**/*.rs for `(debug|info|warn|error)!(... <danger> = %...)` patterns that interpolate raw user prompt / response / content / body into a log macro without redaction (Q5.2)"
  );
  let _ = writeln!(
    sink,
    "  check-arch           assert the runtime-isolation + surface-isolation dependency laws (docs/RFC_CRATE_ARCHITECTURE.md §7); fail on any new cross-edge or stale allowlist entry (P-A0.2)"
  );
  let _ = writeln!(
    sink,
    "  println-lint         fail if agentflow-core/agentflow-nodes/agentflow-nodes-ai contain println!/eprintln! used as logging outside test code (V1.7); suppress a documented exception with `// allow-println-lint: <reason>`"
  );
}

pub(crate) fn workspace_root() -> PathBuf {
  // `CARGO_MANIFEST_DIR` for the xtask crate is `<workspace>/xtask`.
  let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
  manifest_dir
    .parent()
    .map(PathBuf::from)
    .unwrap_or(manifest_dir)
}

pub(crate) fn read_workspace_members(workspace_root: &Path) -> Result<Vec<String>> {
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
