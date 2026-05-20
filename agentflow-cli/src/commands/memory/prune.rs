//! `agentflow memory prune` — drop memory-store rows older than a
//! retention cutoff (P10.7.1).
//!
//! Two layers wired today:
//! - `preference`: calls
//!   [`agentflow_memory::PreferenceStore::prune_older_than`]. Drops
//!   rows whose `updated_at` is older than the cutoff.
//! - `entity_facts`: calls
//!   [`agentflow_memory::EntityFactStore::prune_invalidated`].
//!   Only INVALIDATED rows older than the cutoff are dropped — active
//!   facts are never touched, even when the cutoff is 0.
//!
//! Session + semantic layers don't expose retention-based prune
//! today (their `clear`/`prune` methods are per-session). They'll
//! join this command once the trait surface gains a matching method.

use std::path::PathBuf;
use std::time::Duration;

use agentflow_memory::{
  EntityFactStore, PreferenceStore, SqliteEntityFactStore, SqlitePreferenceStore,
};
use anyhow::{Context, Result, bail};
use colored::Colorize;
use serde_json::json;

/// Execute `agentflow memory prune`.
///
/// `db` is the SQLite file path that backs the chosen layer. The
/// CLI doesn't yet support multiple layers in a single file (each
/// `Sqlite*Store::open(path)` builds its own schema and would
/// silently coexist), so operators pass the same `--db` value used
/// by their agent runtime configuration.
pub async fn execute(
  layer: String,
  db_path: PathBuf,
  older_than: String,
  format: String,
) -> Result<()> {
  let is_envelope = format == "json-envelope";
  let cutoff = parse_retention_duration(&older_than)
    .with_context(|| format!("--older-than '{older_than}' is not a valid duration"))?;

  if !db_path.exists() {
    bail!(
      "memory db '{}' does not exist. Pass --db <path> pointing at the SQLite file your agent \
       runtime writes; nothing to prune.",
      db_path.display()
    );
  }

  let (label, removed) = match layer.as_str() {
    "preference" => {
      let mut store = SqlitePreferenceStore::open(&db_path)
        .await
        .with_context(|| format!("opening preference store at {}", db_path.display()))?;
      let removed = store
        .prune_older_than(cutoff)
        .await
        .context("pruning preference store")?;
      ("preference", removed)
    }
    "entity_facts" => {
      let mut store = SqliteEntityFactStore::open(&db_path)
        .await
        .with_context(|| format!("opening entity_facts store at {}", db_path.display()))?;
      let removed = store
        .prune_invalidated(cutoff)
        .await
        .context("pruning entity_facts store")?;
      ("entity_facts", removed)
    }
    other => bail!(
      "unsupported --layer '{other}'. Supported: preference, entity_facts. \
       Session + semantic layers expose per-session clear instead of retention-based prune \
       and are out of scope for this command."
    ),
  };

  let payload = json!({
    "layer": label,
    "db": db_path.display().to_string(),
    "older_than": older_than,
    "older_than_seconds": cutoff.as_secs(),
    "removed_rows": removed,
  });

  if is_envelope {
    let envelope = crate::json_envelope::CliJsonEnvelope::ok("memory prune", &payload);
    println!("{}", serde_json::to_string_pretty(&envelope)?);
  } else if removed == 0 {
    println!(
      "{} {} {}",
      "✓".green(),
      format!("memory prune ({label})").bold(),
      "— no rows older than the cutoff".dimmed(),
    );
  } else {
    println!(
      "{} {} {}",
      "✓".green(),
      format!("memory prune ({label})").bold(),
      format!("— removed {removed} row(s) older than {older_than}").dimmed(),
    );
  }

  Ok(())
}

/// Parse a retention duration of the form `<integer><unit>` where
/// `unit ∈ {s, m, h, d, w, y}`. Retention windows for memory layers
/// are typically days / weeks / years, so this parser deliberately
/// supports longer units than the workflow-level `parse_duration`
/// in `commands::workflow::run` (which tops out at `m` for minutes).
///
/// A bare integer with no unit is rejected — silently choosing a
/// unit would hide a typo that costs the operator real rows.
pub fn parse_retention_duration(raw: &str) -> Result<Duration> {
  let raw = raw.trim();
  if raw.is_empty() {
    bail!("duration must not be empty");
  }
  // Find the boundary between digits and the unit suffix. The unit
  // is 1-2 lowercase ASCII letters; we don't try to be locale-aware.
  let split = raw.find(|c: char| !c.is_ascii_digit()).ok_or_else(|| {
    anyhow::anyhow!("duration must end in a unit (s, m, h, d, w, y), got `{raw}`")
  })?;
  let (num_part, unit_part) = raw.split_at(split);
  let amount: u64 = num_part
    .parse()
    .with_context(|| format!("duration '{raw}' must start with a non-negative integer"))?;
  let unit = unit_part.trim();
  let multiplier: u64 = match unit {
    "s" => 1,
    "m" => 60,
    "h" => 3_600,
    "d" => 86_400,
    "w" => 604_800,
    // Use 365.25 × 86 400 = 31 557 600 to track the Julian year so
    // long retention windows don't drift over multi-year spans.
    "y" => 31_557_600,
    other => bail!("unknown duration unit '{other}' in '{raw}'. Use one of: s, m, h, d, w, y."),
  };
  Ok(Duration::from_secs(amount.saturating_mul(multiplier)))
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn parse_retention_duration_handles_each_unit() {
    // Pin every supported unit so a regression that drops one (or
    // swaps the multiplier) surfaces on the next run.
    assert_eq!(
      parse_retention_duration("30s").unwrap(),
      Duration::from_secs(30)
    );
    assert_eq!(
      parse_retention_duration("5m").unwrap(),
      Duration::from_secs(5 * 60)
    );
    assert_eq!(
      parse_retention_duration("2h").unwrap(),
      Duration::from_secs(2 * 3600)
    );
    assert_eq!(
      parse_retention_duration("7d").unwrap(),
      Duration::from_secs(7 * 86400)
    );
    assert_eq!(
      parse_retention_duration("4w").unwrap(),
      Duration::from_secs(4 * 604_800)
    );
    // 1 Julian year = 365.25 days = 31 557 600 seconds.
    assert_eq!(
      parse_retention_duration("1y").unwrap(),
      Duration::from_secs(31_557_600)
    );
  }

  #[test]
  fn parse_retention_duration_rejects_bare_integer() {
    // Silently choosing a unit (e.g. defaulting to seconds) would
    // turn `--older-than 30` from "30 seconds" or "30 days?" into a
    // footgun. Pin the explicit-unit invariant.
    let err = parse_retention_duration("30").expect_err("bare integer must error");
    assert!(format!("{err:?}").contains("must end in a unit"), "{err:?}");
  }

  #[test]
  fn parse_retention_duration_rejects_unknown_unit() {
    let err = parse_retention_duration("30mo").expect_err("unknown unit must error");
    assert!(
      format!("{err:?}").contains("unknown duration unit"),
      "{err:?}"
    );
    // The error must name the supported units so the operator can
    // pick the right one without consulting the docs.
    assert!(format!("{err:?}").contains("s, m, h, d, w, y"), "{err:?}");
  }

  #[test]
  fn parse_retention_duration_rejects_empty() {
    let err = parse_retention_duration("   ").expect_err("empty must error");
    assert!(format!("{err:?}").contains("must not be empty"), "{err:?}");
  }

  #[test]
  fn parse_retention_duration_accepts_zero() {
    // A zero-cutoff is meaningful for entity_facts:
    // "prune every invalidated row, regardless of age". Pin so a
    // future refactor doesn't accidentally introduce a positive-only
    // check.
    assert_eq!(parse_retention_duration("0s").unwrap(), Duration::ZERO);
    assert_eq!(parse_retention_duration("0d").unwrap(), Duration::ZERO);
  }

  /// P10.7.1 round-trip: confirms the parser + store wiring agree
  /// end-to-end. The test inserts an "old" row, sleeps past the
  /// chosen cutoff, inserts a "fresh" row, then prunes with that
  /// cutoff. The cutoff (1 second) is the smallest the parser
  /// supports, so the sleep is just over it to avoid clock-drift
  /// flakiness.
  ///
  /// Why not `0s`: with cutoff = 0, the SQL becomes
  /// `updated_at < now()`, which catches the "fresh" row too once
  /// any time at all has elapsed between its insert and the prune
  /// call. A 1s cutoff with a 1.5s sleep gives a clean window where
  /// the old row is strictly older and the fresh row strictly
  /// newer.
  ///
  /// `pool` is private on `SqlitePreferenceStore` so the direct
  /// `UPDATE ... datetime('now', '-10 days')` trick the memory
  /// crate's own test uses isn't available from this crate; this
  /// sleep-then-prune approach is the cleanest equivalent that
  /// stays inside the public API surface.
  #[tokio::test]
  async fn preference_prune_round_trip_removes_old_keeps_fresh() {
    use agentflow_memory::{PreferenceScope, PreferenceStore, SqlitePreferenceStore};

    let mut store = SqlitePreferenceStore::in_memory().await.unwrap();
    let scope = PreferenceScope::local("alice");
    store
      .put_preference(&scope, "theme", serde_json::json!("dark"))
      .await
      .unwrap();

    // Sleep just past the 1s cutoff so `theme.updated_at` lands
    // strictly before `now() - 1s` while `lang.updated_at` is
    // strictly after it.
    tokio::time::sleep(Duration::from_millis(1_500)).await;

    store
      .put_preference(&scope, "lang", serde_json::json!("en"))
      .await
      .unwrap();

    let removed = store
      .prune_older_than(parse_retention_duration("1s").unwrap())
      .await
      .unwrap();
    assert_eq!(removed, 1, "exactly one row (theme) should be pruned");

    let remaining = store.list_preferences(&scope).await.unwrap();
    let keys: Vec<&str> = remaining.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, vec!["lang"], "fresh row must survive");
  }
}
