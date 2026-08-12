//! `check-arch` (P-A0.2, kernel-isolation added R1.2) — assert the subset of
//! the eight crate-dependency laws (`docs/RFC_CRATE_ARCHITECTURE.md` §7)
//! checkable today: runtime-isolation, surface-isolation, and
//! kernel-isolation (R1.2 — an L0 contract crate must not depend on anything
//! outside the L0 kernel set). Known current violations live in
//! `ARCH_ALLOWLIST` with a P-A burndown task; the gate fails on any NEW
//! violation or any stale allowlist entry, so the list can only shrink.
//!
//! Enforce the subset of the eight crate-dependency laws from
//! `docs/RFC_CRATE_ARCHITECTURE.md` §7 that is checkable against the *current*
//! crate set. Three laws are active today:
//!
//!   - runtime-isolation (RFC §7 Law 4/6): a runtime crate must not depend on
//!     another runtime crate. Runtimes today = { core (executor),
//!     agents (loop), harness (shell) }.
//!   - surface-isolation (RFC §10 P-A2): a surface binary crate must not depend
//!     on another surface binary crate. Surfaces = { cli, server, worker }.
//!   - kernel-isolation (RFC §7 Law 1, R1.2 2026-07-28): an L0 contract-kernel
//!     crate must not depend on anything outside the kernel set. Kernel today
//!     = { value, graph, store-spi, agent-spi, async-util, tool } — the crate
//!     list CLAUDE.md's "L0 Contract Kernel" section names (`tool`, not
//!     `tools`, since T3.3 2026-07-30 split the `Tool` contract out of the
//!     builtin-impl-carrying `agentflow-tools`). Added after an
//!     independent audit found `agentflow-agent-spi` depending directly on
//!     `agentflow-llm` (an L2 impl crate) for over a month with neither the
//!     allowlist nor the latent-edge map ever noticing, because until R1.2 no
//!     law covered kernel crates at all (see R1.1 in TODOs.md for the fix that
//!     paid that specific edge down first). Intra-kernel edges (e.g.
//!     `graph -> value`, `agent-spi -> store-spi`) are the intended shape of
//!     the narrow waist and are NOT violations — only an edge that leaves the
//!     kernel set entirely breaks this law.
//!
//! Every edge that breaks an active law must either be FIXED or recorded in
//! `ARCH_ALLOWLIST` with the P-A task that burns it down. The gate FAILS on:
//!   (a) any violating edge NOT in the allowlist — a NEW regression; and
//!   (b) any allowlist entry that is now stale (its edge is gone or no longer
//!       violates a law) — forcing the allowlist to shrink as the migration
//!       pays each edge down.
//!
//! Activating a new law is a one-line change: add the crate set + a
//! `classify_arch_edge` clause (kernel-isolation above is the reference
//! example). Only `[dependencies]` and `[build-dependencies]` count;
//! `[dev-dependencies]` are test-only and do not shape the shipped dependency
//! graph, so they are intentionally excluded.

use crate::read_workspace_members;
use anyhow::{Context, Result, bail};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;

/// Runtime-tier crates (RFC §3). No runtime may depend on another runtime.
const ARCH_RUNTIME_CRATES: &[&str] = &["agentflow-core", "agentflow-agents", "agentflow-harness"];

/// Surface-tier binary crates (RFC §3). No surface may depend on another
/// surface — they compose only via shared contract / assembly crates.
const ARCH_SURFACE_CRATES: &[&str] = &["agentflow-cli", "agentflow-server", "agentflow-worker"];

/// L0 contract-kernel crates (RFC §4, CLAUDE.md "L0 Contract Kernel"). A
/// kernel crate may depend on other kernel crates (that's the narrow waist
/// working as intended) but never on an L2/L3/L4 crate.
///
/// T3.3 (2026-07-30, `docs/RFC_TOOL_CONTRACT_SPLIT.md`): `agentflow-tools`
/// replaced by `agentflow-tool` — the former bundled five concrete builtin
/// tools + four OS-sandbox backends, which is exactly the impl-tier code a
/// kernel crate must never hold (RFC §7 Law 1). The `Tool` contract itself
/// (trait, `ToolRegistry`, `ToolMetadata`, `Capability`, `ToolPolicy`,
/// `SecurityProfile`, the `SandboxBackend` trait + DTOs) now lives in the
/// dependency-free `agentflow-tool`; `agentflow-tools` (the builtin impls)
/// dropped out of the kernel set and re-exports the contract crate in full.
const ARCH_KERNEL_CRATES: &[&str] = &[
  "agentflow-value",
  "agentflow-graph",
  "agentflow-store-spi",
  "agentflow-agent-spi",
  "agentflow-async-util",
  "agentflow-tool",
];

const LAW_RUNTIME_ISOLATION: &str = "runtime-isolation (RFC §7 Law 4/6)";
const LAW_SURFACE_ISOLATION: &str = "surface-isolation (RFC §10 P-A2)";
const LAW_KERNEL_ISOLATION: &str = "kernel-isolation (RFC §7 Law 1)";

/// A currently-tolerated dependency-law violation paired with the P-A
/// migration task that removes it. Each entry must correspond to a real edge
/// that breaks a real law today; the staleness check fails the gate when that
/// stops being true, so the list can only shrink.
struct ArchAllow {
  from: &'static str,
  to: &'static str,
  burndown: &'static str,
}

const ARCH_ALLOWLIST: &[ArchAllow] = &[
  // EMPTY — every tracked runtime/surface-isolation violation has been burned
  // down by the P-A track:
  // - P-A1.3/1.4 + P-A (this): agents -> core. agents builds on the graph IR +
  //   the FlowRunner contract + async-util; the executor (`CoreFlowRunner`) is
  //   injected by the surface, and core is only a dev-dependency.
  // - P-A2.1: harness -> agents. harness depends on the agentflow-agent-spi
  //   contract; agents stays a harness dev-dependency for the smoke test.
  // - P-A2.3: worker -> server. the worker protocol + gRPC client moved to
  //   `agentflow-worker-proto`; server stays a worker dev-dependency for tests.
  // - P-A2.4: server -> cli. the config/executor assembly + the diagnostics
  //   report builder moved to `agentflow-config`.
];

/// A latent target-state violation: an edge that does NOT break either of the
/// two *active* laws (runtime-/surface-isolation) but WILL break a contract-tier
/// law (RFC §7 laws 1/2/3/7) once the kernel crates land and that law is
/// activated. This is the full repoint checklist from
/// `docs/ARCHITECTURE_EVALUATION_2026-06-20.md` §2 (rows 5–11 + `tracing→core`),
/// expanded to individual `from -> to` pairs — the complete target-state edge
/// map, code-tracked so it cannot rot (P-A0.4 / evaluation R5).
///
/// The gate self-maintains the list: it FAILS when a latent edge has been paid
/// down (the dep is gone → prune it) or has become an *active* violation (the
/// edge now breaks an enforced law → move it to `ARCH_ALLOWLIST`). It does not
/// fail merely because the edge still exists — that is the expected state until
/// its kernel crate lands.
struct ArchLatent {
  from: &'static str,
  to: &'static str,
  /// The contract-tier law this edge will break once that law is activated.
  becomes: &'static str,
  /// The P-A task that repoints (pays down) this edge.
  burndown: &'static str,
}

const ARCH_LATENT_EDGES: &[ArchLatent] = &[
  // Row 5 — `agents` runtime fused to concrete impls (law 4); inject via
  // agent-spi / store-spi / tool contracts at surfaces (P-A1.1/1.2 + P-A2.1).
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-llm",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — inject LLM via agent-spi at surfaces",
  },
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-mcp",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — inject MCP tools via tool contract",
  },
  // U2.5 (2026-07-31): `ProjectMemoryStore`/`ProjectFact` PAID DOWN —
  // extracted to `agentflow-store-spi` (mirroring `TaskSummaryStore`),
  // `agentflow-memory` keeps `InMemoryProjectMemoryStore`/
  // `SqliteProjectMemoryStore` and re-exports the contract types under
  // their original paths.
  // U2.6 (2026-08-01): `PreferenceStore` ALSO PAID DOWN — re-auditing
  // found the `&mut self` write methods weren't actually load-bearing
  // (`SqlitePreferenceStore` only ever touches `&self.pool`, an
  // `Arc`-backed `sqlx::SqlitePool` that's already safe to share;
  // `AgeEncryptedPreferenceStore` just encrypts and forwards). Redesigned
  // the trait to `&self`, extracted it to `agentflow-store-spi` alongside
  // `PreferenceScope`/`PreferenceValue`, and `ReActAgent`'s
  // `preference_store` field + `RememberPreferenceTool` both dropped
  // their `Mutex` wrapper for a bare `Arc<dyn PreferenceStore>` —
  // matching `project_memory_store`/`task_summary_store` exactly.
  // This edge STILL stays real (not paid down) — a third, different
  // reason surfaced during U2.6's audit: `agentflow-agents/src/dynamic.rs`
  // (`DynamicWorkflowAgent`, compiling an LLM-authored `agent` plan step)
  // constructs a concrete `Box::new(SessionMemory::default_window())` as
  // its default memory backend. `SessionMemory` has no store-spi contract
  // *by design* (it's a concrete impl, not a missing contract, unlike
  // `ProjectMemoryStore`/`PreferenceStore` were) — closing this edge for
  // real would need `DynamicWorkflowAgent`'s default memory to become
  // caller-injectable, a genuinely different (and larger) change than
  // "extract one more contract." Not attempted in U2.6; no new item
  // opened to track it — left as an accepted, load-bearing use of a
  // concrete `agentflow-memory` type, not a to-do.
  ArchLatent {
    from: "agentflow-agents",
    to: "agentflow-memory",
    becomes: "law 4 runtime→impl",
    burndown: "not closable via contract extraction alone — dynamic.rs's SessionMemory default needs an injectable-memory redesign",
  },
  // T3.3 (2026-07-30): `agents -> tools` PAID DOWN — `agentflow-tools`
  // split into `agentflow-tool` (contract) + `agentflow-tools` (builtin
  // impl), and `agentflow-agents` now depends on the contract crate only
  // (`docs/RFC_TOOL_CONTRACT_SPLIT.md`).
  // Row 6 — `harness` carries 5 impl edges; only `harness→agents` is in the
  // allowlist. These three remain after P-A2.1 repoints harness→agent-spi.
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-llm",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.2 — tokenizer via value/store-spi util (R6)",
  },
  // T3.3 (2026-07-30): `harness -> tools` PAID DOWN alongside `agents ->
  // tools` above — `agentflow-harness` now depends on the `agentflow-tool`
  // contract crate only.
  // U2.1 (2026-07-30): `harness -> memory` PAID DOWN — production code
  // only ever touched `MemoryStore`/`Message` (both plain store-spi
  // re-exports), so `[dependencies]` now points at `agentflow-store-spi`
  // directly; `agentflow-memory` moved to `[dev-dependencies]` for
  // `SessionMemory` in this crate's own tests. `agents -> memory` (row
  // above) stays real, not paid down — see the U2.5/U2.6 notes on that row for
  // the current (post-U2.2 preference wiring) reason why.
  ArchLatent {
    from: "agentflow-harness",
    to: "agentflow-tracing",
    becomes: "law 4 runtime→impl",
    burndown: "P-A1.1 — redaction/trace-context via agent-spi (R6)",
  },
  // Row 7–8 — `nodes` straddler. P-A0.5 BURNED DOWN the capability edges
  // (nodes→{llm,rag,mcp}): the capability-backed nodes moved to the new
  // `agentflow-nodes-ai` adapter crate, so the tool-tier `agentflow-nodes`
  // crate carries no capability deps. The IR edge below remains.
  ArchLatent {
    from: "agentflow-nodes",
    to: "agentflow-core",
    becomes: "law 2 tool→runtime",
    burndown: "P-A1.3 — IR-only edge; becomes nodes→graph",
  },
  // Row 9 — `skills` capability depends on the `agents` runtime (law 3 inversion).
  ArchLatent {
    from: "agentflow-skills",
    to: "agentflow-agents",
    becomes: "law 3 capability→runtime",
    burndown: "P-A4.3 — Capability::lower; surface wires the runtime",
  },
  // Row 10 — `memory` capability→capability (law 3).
  ArchLatent {
    from: "agentflow-memory",
    to: "agentflow-rag",
    becomes: "law 3 capability→capability",
    burndown: "P-A1.2 — EmbeddingProvider via store-spi (R6)",
  },
  // Row 11 — `mcp` tool→ops (law 2), traceparent ambient only.
  ArchLatent {
    from: "agentflow-mcp",
    to: "agentflow-tracing",
    becomes: "law 2 tool→ops",
    burndown: "P-A1.1 — trace-context contract via agent-spi/value (R6)",
  },
  // Extra — `tracing` ops→runtime for the workflow event types.
  ArchLatent {
    from: "agentflow-tracing",
    to: "agentflow-core",
    becomes: "ops→runtime",
    burndown: "P-A1.1/P-A1.5 — depend on agent-spi + value, not core",
  },
];

/// Return the law a `from -> to` internal edge breaks, or `None` when the edge
/// is allowed. Pure over the supplied tier sets so it is unit-testable with
/// synthetic crate names.
fn classify_arch_edge(
  from: &str,
  to: &str,
  runtimes: &[&str],
  surfaces: &[&str],
  kernels: &[&str],
) -> Option<&'static str> {
  let member = |set: &[&str], c: &str| set.contains(&c);
  if member(runtimes, from) && member(runtimes, to) {
    return Some(LAW_RUNTIME_ISOLATION);
  }
  if member(surfaces, from) && member(surfaces, to) {
    return Some(LAW_SURFACE_ISOLATION);
  }
  if member(kernels, from) && !member(kernels, to) {
    return Some(LAW_KERNEL_ISOLATION);
  }
  None
}

/// Outcome of evaluating the architecture laws over a set of edges.
struct ArchEval {
  /// Violating edges recorded in the allowlist (tolerated debt).
  tracked: Vec<(String, String, &'static str)>,
  /// Violating edges NOT in the allowlist (new regressions).
  new: Vec<(String, String, &'static str)>,
  /// Allowlist `(from, to)` pairs whose edge is gone or no longer violates.
  stale: Vec<(String, String)>,
}

/// Pure evaluator: classify every edge, split into tracked vs new violations,
/// and flag stale allowlist entries. No filesystem access, so it is unit-
/// tested directly with synthetic inputs.
fn evaluate_arch(
  edges: &[(String, String)],
  runtimes: &[&str],
  surfaces: &[&str],
  kernels: &[&str],
  allowlist: &[(&str, &str)],
) -> ArchEval {
  let allow: BTreeSet<(&str, &str)> = allowlist.iter().copied().collect();
  let edge_set: BTreeSet<(&str, &str)> = edges
    .iter()
    .map(|(a, b)| (a.as_str(), b.as_str()))
    .collect();

  let mut tracked = Vec::new();
  let mut new = Vec::new();
  for (from, to) in edges {
    if let Some(law) = classify_arch_edge(from, to, runtimes, surfaces, kernels) {
      if allow.contains(&(from.as_str(), to.as_str())) {
        tracked.push((from.clone(), to.clone(), law));
      } else {
        new.push((from.clone(), to.clone(), law));
      }
    }
  }

  let mut stale = Vec::new();
  for (from, to) in allowlist {
    let present = edge_set.contains(&(*from, *to));
    let violates = classify_arch_edge(from, to, runtimes, surfaces, kernels).is_some();
    if !present || !violates {
      stale.push((from.to_string(), to.to_string()));
    }
  }

  ArchEval {
    tracked,
    new,
    stale,
  }
}

/// Outcome of evaluating the latent target-state edge map (`ARCH_LATENT_EDGES`).
struct LatentEval {
  /// Latent edges that still exist and are not yet active violations (expected).
  present: Vec<(String, String, &'static str)>,
  /// Latent entries whose edge is gone — paid down; prune from the list.
  resolved: Vec<(String, String)>,
  /// Latent entries whose edge now breaks an *active* law — move to ARCH_ALLOWLIST.
  misfiled: Vec<(String, String, &'static str)>,
}

/// Pure evaluator for the latent edge map. Pure over its inputs so it is
/// unit-tested directly with synthetic crate names. A latent entry is healthy
/// while its edge exists and is not yet classified as an active violation;
/// `resolved` (edge gone) and `misfiled` (edge now actively violates) both force
/// the list to be updated, so the map can only stay truthful or shrink.
fn evaluate_latent(
  edges: &[(String, String)],
  latent: &[(&str, &str, &'static str)],
  runtimes: &[&str],
  surfaces: &[&str],
  kernels: &[&str],
) -> LatentEval {
  let edge_set: BTreeSet<(&str, &str)> = edges
    .iter()
    .map(|(a, b)| (a.as_str(), b.as_str()))
    .collect();
  let mut present = Vec::new();
  let mut resolved = Vec::new();
  let mut misfiled = Vec::new();
  for (from, to, becomes) in latent {
    if !edge_set.contains(&(*from, *to)) {
      resolved.push((from.to_string(), to.to_string()));
    } else if let Some(law) = classify_arch_edge(from, to, runtimes, surfaces, kernels) {
      misfiled.push((from.to_string(), to.to_string(), law));
    } else {
      present.push((from.to_string(), to.to_string(), *becomes));
    }
  }
  LatentEval {
    present,
    resolved,
    misfiled,
  }
}

/// Read the internal (workspace-member) dependencies declared by `manifest`.
/// Considers `[dependencies]` + `[build-dependencies]`; resolves renamed deps
/// via their `package = "..."` key. `[dev-dependencies]` are excluded by
/// design — they are test-only and do not shape the shipped graph.
fn read_internal_deps(manifest: &Path, members: &BTreeSet<String>) -> Result<Vec<String>> {
  let content = std::fs::read_to_string(manifest)
    .with_context(|| format!("Failed to read {}", manifest.display()))?;
  let parsed: toml::Value =
    toml::from_str(&content).with_context(|| format!("Failed to parse {}", manifest.display()))?;
  let mut deps: BTreeSet<String> = BTreeSet::new();
  for table in ["dependencies", "build-dependencies"] {
    let Some(tbl) = parsed.get(table).and_then(|t| t.as_table()) else {
      continue;
    };
    for (key, value) in tbl {
      // `foo = { package = "agentflow-x" }` renames resolve to the real crate.
      let crate_name = value
        .as_table()
        .and_then(|t| t.get("package"))
        .and_then(|p| p.as_str())
        .unwrap_or(key.as_str());
      if members.contains(crate_name) {
        deps.insert(crate_name.to_string());
      }
    }
  }
  Ok(deps.into_iter().collect())
}

/// Build the internal dependency edge list for the whole workspace.
fn collect_arch_edges(workspace_root: &Path) -> Result<Vec<(String, String)>> {
  let members = read_workspace_members(workspace_root)?;
  let member_set: BTreeSet<String> = members.iter().cloned().collect();
  let mut edges: Vec<(String, String)> = Vec::new();
  for member in &members {
    let manifest = workspace_root.join(member).join("Cargo.toml");
    if !manifest.exists() {
      continue;
    }
    for dep in read_internal_deps(&manifest, &member_set)? {
      edges.push((member.clone(), dep));
    }
  }
  edges.sort();
  edges.dedup();
  Ok(edges)
}

/// Run the architecture-law gate against `workspace_root` and report through
/// the caller-supplied sinks. Returns `Ok(())` only when there are zero new
/// violations and zero stale allowlist entries.
pub(crate) fn check_arch_at(
  workspace_root: &Path,
  out: &mut impl Write,
  err: &mut impl Write,
) -> Result<()> {
  let members = read_workspace_members(workspace_root)?;
  let edges = collect_arch_edges(workspace_root)?;

  let allow_pairs: Vec<(&str, &str)> = ARCH_ALLOWLIST.iter().map(|a| (a.from, a.to)).collect();
  let eval = evaluate_arch(
    &edges,
    ARCH_RUNTIME_CRATES,
    ARCH_SURFACE_CRATES,
    ARCH_KERNEL_CRATES,
    &allow_pairs,
  );

  let latent_pairs: Vec<(&str, &str, &'static str)> = ARCH_LATENT_EDGES
    .iter()
    .map(|l| (l.from, l.to, l.becomes))
    .collect();
  let latent = evaluate_latent(
    &edges,
    &latent_pairs,
    ARCH_RUNTIME_CRATES,
    ARCH_SURFACE_CRATES,
    ARCH_KERNEL_CRATES,
  );

  writeln!(
    out,
    "check-arch: {} member(s), {} internal edge(s), 3 active law(s)",
    members.len(),
    edges.len()
  )?;
  writeln!(
    out,
    "check-arch: {} tracked (allowlisted), {} new, {} stale allowlist entr(ies)",
    eval.tracked.len(),
    eval.new.len(),
    eval.stale.len()
  )?;
  for (from, to, law) in &eval.tracked {
    writeln!(out, "  · tracked: {from} -> {to} breaks {law}")?;
  }

  // Latent target-state map (P-A0.4): informational until each contract-tier
  // law is activated; the repoint checklist for the kernel migration.
  writeln!(
    out,
    "check-arch: {} latent target-state edge(s) (not yet enforced; see docs/ARCHITECTURE_EVALUATION_2026-06-20.md §2)",
    latent.present.len()
  )?;
  for (from, to, becomes) in &latent.present {
    writeln!(out, "  ◦ latent: {from} -> {to} will break {becomes}")?;
  }

  if eval.new.is_empty()
    && eval.stale.is_empty()
    && latent.resolved.is_empty()
    && latent.misfiled.is_empty()
  {
    writeln!(out, "check-arch: OK")?;
    return Ok(());
  }

  writeln!(err, "check-arch: FAIL")?;
  for (from, to, law) in &eval.new {
    writeln!(
      err,
      "  ✗ NEW violation: {from} -> {to} breaks {law} — fix it or add to ARCH_ALLOWLIST with a burndown task"
    )?;
  }
  for (from, to) in &eval.stale {
    let note = ARCH_ALLOWLIST
      .iter()
      .find(|a| a.from == from && a.to == to)
      .map(|a| a.burndown)
      .unwrap_or("(no burndown recorded)");
    writeln!(
      err,
      "  ✗ STALE allowlist: {from} -> {to} no longer violates — remove it from ARCH_ALLOWLIST (burndown: {note})"
    )?;
  }
  for (from, to) in &latent.resolved {
    let note = ARCH_LATENT_EDGES
      .iter()
      .find(|l| l.from == from && l.to == to)
      .map(|l| l.burndown)
      .unwrap_or("(no burndown recorded)");
    writeln!(
      err,
      "  ✗ RESOLVED latent: {from} -> {to} edge is gone — remove it from ARCH_LATENT_EDGES (paid down: {note})"
    )?;
  }
  for (from, to, law) in &latent.misfiled {
    writeln!(
      err,
      "  ✗ MISFILED latent: {from} -> {to} now breaks {law} — move it from ARCH_LATENT_EDGES to ARCH_ALLOWLIST"
    )?;
  }
  bail!(
    "{} new, {} stale allowlist, {} resolved latent, {} misfiled latent",
    eval.new.len(),
    eval.stale.len(),
    latent.resolved.len(),
    latent.misfiled.len()
  );
}

#[cfg(test)]
mod arch_tests {
  use super::*;

  fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
      .iter()
      .map(|(a, b)| (a.to_string(), b.to_string()))
      .collect()
  }

  #[test]
  fn runtime_to_runtime_is_a_new_violation() {
    let e = edges(&[("r-a", "r-b")]);
    let eval = evaluate_arch(&e, &["r-a", "r-b"], &[], &[], &[]);
    assert_eq!(eval.new.len(), 1);
    assert_eq!(eval.tracked.len(), 0);
    assert_eq!(eval.stale.len(), 0);
    assert_eq!(eval.new[0].2, LAW_RUNTIME_ISOLATION);
  }

  #[test]
  fn allowlisted_violation_is_tracked_not_new() {
    let e = edges(&[("r-a", "r-b")]);
    let eval = evaluate_arch(&e, &["r-a", "r-b"], &[], &[], &[("r-a", "r-b")]);
    assert_eq!(eval.new.len(), 0);
    assert_eq!(eval.tracked.len(), 1);
    assert_eq!(eval.stale.len(), 0);
  }

  #[test]
  fn surface_to_surface_is_flagged() {
    let e = edges(&[("s-a", "s-b")]);
    let eval = evaluate_arch(&e, &[], &["s-a", "s-b"], &[], &[]);
    assert_eq!(eval.new.len(), 1);
    assert_eq!(eval.new[0].2, LAW_SURFACE_ISOLATION);
  }

  #[test]
  fn non_tier_edges_are_allowed() {
    let e = edges(&[("cap", "tool")]);
    let eval = evaluate_arch(&e, &["r-a"], &["s-a"], &[], &[]);
    assert!(eval.new.is_empty() && eval.tracked.is_empty() && eval.stale.is_empty());
  }

  #[test]
  fn stale_allowlist_when_edge_removed() {
    // The allowlisted edge is no longer in the graph → it must be pruned.
    let eval = evaluate_arch(&[], &["r-a", "r-b"], &[], &[], &[("r-a", "r-b")]);
    assert_eq!(eval.stale, vec![("r-a".to_string(), "r-b".to_string())]);
  }

  #[test]
  fn stale_allowlist_when_edge_no_longer_violates() {
    // Edge still present but neither endpoint is a runtime/surface → no law
    // broken, so the allowlist entry is pointless and flagged stale.
    let e = edges(&[("plain-a", "plain-b")]);
    let eval = evaluate_arch(&e, &["r-a"], &[], &[], &[("plain-a", "plain-b")]);
    assert_eq!(eval.stale.len(), 1);
    assert_eq!(eval.new.len(), 0);
  }

  #[test]
  fn kernel_depending_on_non_kernel_is_a_new_violation() {
    // R1.2 regression pin: this is exactly the shape of the bug the audit
    // found (agentflow-agent-spi -> agentflow-llm) before R1.1 fixed it.
    let e = edges(&[("k-a", "impl-x")]);
    let eval = evaluate_arch(&e, &[], &[], &["k-a", "k-b"], &[]);
    assert_eq!(eval.new.len(), 1);
    assert_eq!(eval.new[0].2, LAW_KERNEL_ISOLATION);
  }

  #[test]
  fn kernel_depending_on_kernel_is_allowed() {
    // Intra-kernel edges are the narrow waist working as intended, e.g.
    // `agentflow-graph -> agentflow-value` or `agent-spi -> store-spi`.
    let e = edges(&[("k-a", "k-b")]);
    let eval = evaluate_arch(&e, &[], &[], &["k-a", "k-b"], &[]);
    assert!(eval.new.is_empty() && eval.tracked.is_empty() && eval.stale.is_empty());
  }

  #[test]
  fn non_kernel_depending_on_kernel_is_allowed() {
    // The normal direction — any L1+ crate depending on a kernel contract
    // crate is exactly what the kernel is for.
    let e = edges(&[("impl-x", "k-a")]);
    let eval = evaluate_arch(&e, &[], &[], &["k-a", "k-b"], &[]);
    assert!(eval.new.is_empty() && eval.tracked.is_empty() && eval.stale.is_empty());
  }

  #[test]
  fn latent_edge_present_is_reported_not_failed() {
    // A latent edge that exists and breaks no *active* law is healthy: it shows
    // up in `present` and never fails the gate.
    let e = edges(&[("nodes", "llm")]);
    let l = evaluate_latent(
      &e,
      &[("nodes", "llm", "law 2 tool→capability")],
      &["r-a"],
      &["s-a"],
      &[],
    );
    assert_eq!(l.present.len(), 1);
    assert!(l.resolved.is_empty() && l.misfiled.is_empty());
    assert_eq!(l.present[0].2, "law 2 tool→capability");
  }

  #[test]
  fn latent_edge_gone_is_resolved() {
    // The latent edge was paid down (dep removed) → must be pruned from the list.
    let l = evaluate_latent(&[], &[("nodes", "llm", "law 2")], &["r-a"], &["s-a"], &[]);
    assert_eq!(l.resolved, vec![("nodes".to_string(), "llm".to_string())]);
    assert!(l.present.is_empty() && l.misfiled.is_empty());
  }

  #[test]
  fn latent_edge_that_now_violates_active_law_is_misfiled() {
    // The edge still exists but now breaks an *active* law (both endpoints are
    // runtimes) → it belongs in ARCH_ALLOWLIST, not the latent list.
    let e = edges(&[("r-a", "r-b")]);
    let l = evaluate_latent(
      &e,
      &[("r-a", "r-b", "law 4 runtime→impl")],
      &["r-a", "r-b"],
      &[],
      &[],
    );
    assert_eq!(l.misfiled.len(), 1);
    assert_eq!(l.misfiled[0].2, LAW_RUNTIME_ISOLATION);
    assert!(l.present.is_empty() && l.resolved.is_empty());
  }

  #[test]
  fn read_internal_deps_resolves_members_and_excludes_dev_deps() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = dir.path().join("Cargo.toml");
    std::fs::write(
      &manifest,
      "[package]\nname = \"x\"\nversion = \"0.0.0\"\nedition = \"2024\"\n\n\
       [dependencies]\n\
       agentflow-core = { path = \"../agentflow-core\" }\n\
       aliased = { package = \"agentflow-tools\" }\n\
       serde = \"1\"\n\n\
       [dev-dependencies]\n\
       agentflow-llm = { path = \"../agentflow-llm\" }\n",
    )
    .expect("write manifest");
    let members: BTreeSet<String> = ["agentflow-core", "agentflow-tools", "agentflow-llm"]
      .iter()
      .map(|s| s.to_string())
      .collect();
    let deps = read_internal_deps(&manifest, &members).expect("read deps");
    assert!(deps.contains(&"agentflow-core".to_string()));
    assert!(
      deps.contains(&"agentflow-tools".to_string()),
      "rename via package= must resolve"
    );
    assert!(
      !deps.contains(&"agentflow-llm".to_string()),
      "dev-dependencies must be excluded"
    );
    assert_eq!(deps.len(), 2);
  }

  #[test]
  fn real_workspace_passes_with_current_allowlist() {
    // Self-consistency guard: the real workspace must be clean under the gate
    // with exactly the seeded allowlist. Fails CI when someone adds a NEW
    // runtime/surface cross-edge, or FIXES one without pruning the allowlist.
    let root = crate::workspace_root();
    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let result = check_arch_at(&root, &mut out, &mut err);
    assert!(
      result.is_ok(),
      "real workspace failed check-arch:\nstdout:\n{}\nstderr:\n{}",
      String::from_utf8_lossy(&out),
      String::from_utf8_lossy(&err),
    );
    let stdout = String::from_utf8(out).expect("utf8 stdout");
    assert!(stdout.contains("check-arch: OK"), "stdout:\n{stdout}");
    assert!(
      stdout.contains("0 tracked"),
      "expected ZERO tracked violations — the entire P-A runtime/surface-isolation \
       allowlist is burned down (agents->core, harness->agents, server->cli, \
       worker->server); got:\n{stdout}"
    );
    assert!(
      stdout.contains("latent target-state edge(s)"),
      "expected the latent target-state map to be reported; got:\n{stdout}"
    );
  }

  #[test]
  fn latent_map_entries_are_unique_and_distinct_from_allowlist() {
    // Guard against a latent edge being listed twice, or being duplicated in
    // both ARCH_LATENT_EDGES and ARCH_ALLOWLIST (the two lists must partition
    // the target-state edge map, not overlap).
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for l in ARCH_LATENT_EDGES {
      assert!(
        seen.insert((l.from, l.to)),
        "duplicate latent edge: {} -> {}",
        l.from,
        l.to
      );
      assert!(
        !ARCH_ALLOWLIST
          .iter()
          .any(|a| a.from == l.from && a.to == l.to),
        "{} -> {} is in BOTH ARCH_LATENT_EDGES and ARCH_ALLOWLIST",
        l.from,
        l.to
      );
    }
  }

  /// T3.3 regression (`docs/RFC_TOOL_CONTRACT_SPLIT.md`): `agentflow-tools`
  /// bundled concrete builtin tools + OS-sandbox backends, which is exactly
  /// the impl-tier code a kernel crate must never hold — it was replaced by
  /// the dependency-free `agentflow-tool` contract crate in the kernel set,
  /// and `agentflow-agents` / `agentflow-harness` now depend on that
  /// contract crate directly instead of the builtin-impl one, resolving
  /// both `law 4 runtime→impl` latent edges this test locks in as gone.
  #[test]
  fn tool_contract_split_removed_the_tools_kernel_membership_and_latent_edges() {
    assert!(
      ARCH_KERNEL_CRATES.contains(&"agentflow-tool"),
      "agentflow-tool must be the kernel-tier contract crate"
    );
    assert!(
      !ARCH_KERNEL_CRATES.contains(&"agentflow-tools"),
      "agentflow-tools (builtin impls) must not be in the kernel set"
    );
    for (from, to) in [
      ("agentflow-agents", "agentflow-tools"),
      ("agentflow-harness", "agentflow-tools"),
    ] {
      assert!(
        !ARCH_LATENT_EDGES
          .iter()
          .any(|l| l.from == from && l.to == to),
        "{from} -> {to} should have been paid down by the T3.3 split, not \
         merely reclassified"
      );
    }
  }
}
