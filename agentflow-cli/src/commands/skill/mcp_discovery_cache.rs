//! Manifest-level cache for `skill inspect` MCP discovery (P10.9.1).
//!
//! Spawning every declared MCP server on every `skill inspect` call is
//! slow — each server is an external subprocess + JSON-RPC handshake.
//! Caching at the **manifest** level (keyed by a stable hash of the
//! `mcp_servers` section) means a re-inspect of the same skill is free
//! when nothing relevant changed. The first invocation pays the
//! discovery cost; later invocations read the persisted tool list.
//!
//! ## Hash inputs
//!
//! The hash deliberately includes only fields that affect what the MCP
//! server advertises:
//!
//! - `name` (the server identifier the policy resolver groups by)
//! - `command` (the binary that actually runs)
//! - `args` (in the order they appear — argv order matters)
//! - `env` (in sorted order so HashMap iteration randomness doesn't
//!   leak into the hash)
//!
//! Timeout / max_concurrent_calls are intentionally excluded — they
//! affect runtime behaviour but not the tool list. If you change them
//! you don't want a re-discovery.
//!
//! ## TTL
//!
//! A 24-hour TTL covers the common case (MCP-advertised tools rarely
//! change day-to-day) without forcing operators to remember to bust
//! the cache after upstream server upgrades. The `--refresh-mcp-cache`
//! flag forces a fresh discovery for those cases.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use agentflow_skills::manifest::McpServerConfig;
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// Cache TTL — entries older than this are treated as stale and
/// trigger a fresh discovery. 24 hours balances "don't refetch every
/// time" against "don't serve year-old tool lists if the upstream
/// server changed".
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

/// Schema version. Bump on any breaking change to [`CacheEntry`] or
/// the hash input format so stale entries from older CLIs are
/// silently dropped rather than mis-interpreted.
const CACHE_SCHEMA_VERSION: u32 = 1;

/// On-disk shape of the cache file. A single JSON document with one
/// entry per manifest hash; keeping it single-file makes wiping the
/// cache (`rm ~/.agentflow/cache/skill_mcp_discovery.json`) trivial.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DiscoveryCache {
  pub version: u32,
  #[serde(default)]
  pub entries: BTreeMap<String, CacheEntry>,
}

/// One cached discovery result. `tools_by_server` mirrors the
/// in-memory `McpCapabilityMap` shape so callers can swap them in
/// without translation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry {
  /// Tool list per declared MCP server.
  pub tools_by_server: BTreeMap<String, Vec<String>>,
  /// When this entry was last written. Used for TTL expiry.
  pub cached_at: DateTime<Utc>,
}

impl DiscoveryCache {
  pub fn new() -> Self {
    Self {
      version: CACHE_SCHEMA_VERSION,
      entries: BTreeMap::new(),
    }
  }

  /// Default cache file location: `~/.agentflow/cache/skill_mcp_discovery.json`.
  /// Returns `None` only when the home directory can't be resolved
  /// (an unusual condition — the CLI uses `~/.agentflow/...` for
  /// every persistent file).
  pub fn default_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| {
      h.join(".agentflow")
        .join("cache")
        .join("skill_mcp_discovery.json")
    })
  }

  /// Load the cache from `path`. Returns an empty cache when the
  /// file doesn't exist or has a schema mismatch — both are normal
  /// states (first run + post-upgrade respectively) and should not
  /// surface as errors to the operator.
  pub fn load(path: &Path) -> Self {
    if !path.exists() {
      return Self::new();
    }
    let raw = match std::fs::read_to_string(path) {
      Ok(s) => s,
      // Read errors are non-fatal — we'd rather pay one discovery
      // cycle than crash the inspect command. The cache will be
      // rebuilt on next save.
      Err(_) => return Self::new(),
    };
    let parsed: Self = match serde_json::from_str(&raw) {
      Ok(c) => c,
      Err(_) => return Self::new(),
    };
    if parsed.version != CACHE_SCHEMA_VERSION {
      return Self::new();
    }
    parsed
  }

  /// Save the cache to `path`, creating parent directories as
  /// needed. Errors propagate — a silent save failure would mean
  /// the next inspect call pays the discovery cost again and the
  /// operator never knows why.
  pub fn save(&self, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
      std::fs::create_dir_all(parent)
        .with_context(|| format!("creating cache directory {}", parent.display()))?;
    }
    let raw = serde_json::to_string_pretty(self).context("serialising cache")?;
    std::fs::write(path, raw).with_context(|| format!("writing cache file {}", path.display()))?;
    Ok(())
  }

  /// Look up an entry by manifest hash; only returns `Some` when the
  /// entry is **fresh** (within `ttl` of now). Stale entries are
  /// surfaced as cache misses so the inspect command refreshes them
  /// transparently.
  pub fn lookup_fresh(&self, hash: &str, ttl: Duration) -> Option<&CacheEntry> {
    let entry = self.entries.get(hash)?;
    let age = (Utc::now() - entry.cached_at).to_std().ok()?;
    if age <= ttl { Some(entry) } else { None }
  }

  /// Insert or replace the entry for `hash`. `cached_at` is stamped
  /// at write time inside this method so callers can't accidentally
  /// poison the cache with a stale timestamp.
  pub fn upsert(&mut self, hash: String, tools_by_server: BTreeMap<String, Vec<String>>) {
    self.entries.insert(
      hash,
      CacheEntry {
        tools_by_server,
        cached_at: Utc::now(),
      },
    );
  }
}

/// Compute a stable hex SHA-256 over the fields of `mcp_servers` that
/// affect what tools each server advertises. Sorted env iteration
/// keeps the hash deterministic across runs (HashMap order is
/// randomised by default).
pub fn hash_mcp_servers(servers: &[McpServerConfig]) -> String {
  // Build a canonical struct so serde gives us a stable ordering
  // (BTreeMap for env, fixed field order for the struct).
  #[derive(Serialize)]
  struct CanonicalServer<'a> {
    name: &'a str,
    command: &'a str,
    args: &'a [String],
    env: BTreeMap<&'a str, &'a str>,
  }

  let canonical: Vec<CanonicalServer<'_>> = servers
    .iter()
    .map(|s| CanonicalServer {
      name: &s.name,
      command: &s.command,
      args: &s.args,
      env: s
        .env
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect(),
    })
    .collect();
  // Sort by server name so re-ordering inside the manifest doesn't
  // bust the cache. Argv order STILL matters per server (it can
  // affect tool advertisements for e.g. CLI-configurable servers),
  // so we don't sort within a server.
  let mut canonical = canonical;
  canonical.sort_by(|a, b| a.name.cmp(b.name));

  // serde_json with sorted maps + stable field order → byte-stable
  // input to the hasher. `to_vec` only fails on non-finite floats
  // (NaN/Inf) or map keys that aren't strings — neither is reachable
  // for the `CanonicalServerView` shape (Vec of structs with String
  // / &str fields only). The `expect` is a build-time invariant.
  #[allow(
    clippy::expect_used,
    reason = "CanonicalServerView has only String/&str fields; serde_json::to_vec cannot fail"
  )]
  let body = serde_json::to_vec(&canonical).expect("canonical hash input must serialise");
  let mut hasher = Sha256::new();
  hasher.update(&body);
  format!("{:x}", hasher.finalize())
}

/// Normalise the in-memory `McpCapabilityMap` for the cache: clone,
/// sort the tool list inside each server entry, dedup. Returning a
/// `BTreeMap` (the same shape `McpCapabilityMap` already is) keeps
/// the on-disk JSON deterministic across runs.
pub fn to_cache_value(caps: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
  let mut out = BTreeMap::new();
  for (server, tools) in caps {
    let mut sorted = tools.clone();
    sorted.sort();
    sorted.dedup();
    out.insert(server.clone(), sorted);
  }
  out
}

/// Read a cache entry back into the in-memory shape the policy
/// resolver consumes (identity in shape, but cloned so the caller
/// owns the map).
pub fn from_cache_value(caps: &BTreeMap<String, Vec<String>>) -> BTreeMap<String, Vec<String>> {
  caps.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

#[cfg(test)]
mod tests {
  use super::*;

  fn server(name: &str, command: &str, args: &[&str], env: &[(&str, &str)]) -> McpServerConfig {
    McpServerConfig {
      name: name.to_string(),
      command: command.to_string(),
      args: args.iter().map(|s| s.to_string()).collect(),
      env: env
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect(),
      timeout_secs: None,
      max_concurrent_calls: None,
    }
  }

  // ── hash_mcp_servers ────────────────────────────────────────────────

  #[test]
  fn hash_mcp_servers_is_stable_across_env_iteration_order() {
    // HashMap iteration is randomised, so two calls with the same
    // env content but constructed differently must still hash the
    // same. This is the invariant that lets the cache actually hit.
    let a = vec![server("p", "node", &["main.js"], &[("X", "1"), ("Y", "2")])];
    let b = vec![server("p", "node", &["main.js"], &[("Y", "2"), ("X", "1")])];
    assert_eq!(hash_mcp_servers(&a), hash_mcp_servers(&b));
  }

  #[test]
  fn hash_mcp_servers_is_stable_across_server_ordering() {
    // Operators sometimes re-order servers inside the manifest for
    // readability — that's not a semantic change, so the cache must
    // not invalidate.
    let a = vec![
      server("alpha", "a", &[], &[]),
      server("beta", "b", &[], &[]),
    ];
    let b = vec![
      server("beta", "b", &[], &[]),
      server("alpha", "a", &[], &[]),
    ];
    assert_eq!(hash_mcp_servers(&a), hash_mcp_servers(&b));
  }

  #[test]
  fn hash_mcp_servers_distinguishes_argv_order() {
    // Argv order matters — `node --foo --bar` and `node --bar --foo`
    // can produce different tool advertisements for some servers.
    // Pin so a "let's sort args" optimisation can't silently land.
    let a = vec![server("p", "node", &["--foo", "--bar"], &[])];
    let b = vec![server("p", "node", &["--bar", "--foo"], &[])];
    assert_ne!(hash_mcp_servers(&a), hash_mcp_servers(&b));
  }

  #[test]
  fn hash_mcp_servers_distinguishes_command_changes() {
    let a = vec![server("p", "node", &["main.js"], &[])];
    let b = vec![server("p", "deno", &["main.js"], &[])];
    assert_ne!(hash_mcp_servers(&a), hash_mcp_servers(&b));
  }

  #[test]
  fn hash_mcp_servers_distinguishes_env_value_changes() {
    let a = vec![server("p", "node", &[], &[("API_KEY", "v1")])];
    let b = vec![server("p", "node", &[], &[("API_KEY", "v2")])];
    assert_ne!(hash_mcp_servers(&a), hash_mcp_servers(&b));
  }

  #[test]
  fn hash_mcp_servers_ignores_timeout_changes() {
    // Timeout/max_concurrent_calls affect runtime, not the tool
    // list. Changing them must NOT invalidate the cache — that's
    // the whole point of the careful hash-input choice.
    let mut a = server("p", "node", &[], &[]);
    a.timeout_secs = Some(10);
    let mut b = server("p", "node", &[], &[]);
    b.timeout_secs = Some(60);
    assert_eq!(hash_mcp_servers(&[a]), hash_mcp_servers(&[b]));
  }

  // ── load/save round-trip ─────────────────────────────────────────────

  #[test]
  fn load_round_trips_via_save() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");

    let mut cache = DiscoveryCache::new();
    let mut tools = BTreeMap::new();
    tools.insert(
      "server-a".to_string(),
      vec!["t1".to_string(), "t2".to_string()],
    );
    cache.upsert("abc123".to_string(), tools);
    cache.save(&path).unwrap();

    let loaded = DiscoveryCache::load(&path);
    let entry = loaded.entries.get("abc123").expect("entry round-trips");
    assert_eq!(entry.tools_by_server["server-a"], vec!["t1", "t2"]);
  }

  #[test]
  fn load_returns_empty_when_file_missing() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nope.json");
    let cache = DiscoveryCache::load(&path);
    assert!(cache.entries.is_empty());
  }

  #[test]
  fn load_returns_empty_on_schema_version_mismatch() {
    // A future schema bump must drop old entries silently rather
    // than fail-closing the inspect command.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");
    std::fs::write(&path, r#"{"version": 999, "entries": {}}"#).unwrap();
    let cache = DiscoveryCache::load(&path);
    assert_eq!(cache.version, CACHE_SCHEMA_VERSION);
    assert!(cache.entries.is_empty());
  }

  #[test]
  fn load_returns_empty_on_malformed_json() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.json");
    std::fs::write(&path, "not json at all").unwrap();
    // The CLI must never crash on a corrupt cache file — the
    // discovery just runs fresh and overwrites.
    let cache = DiscoveryCache::load(&path);
    assert!(cache.entries.is_empty());
  }

  // ── TTL lookup ───────────────────────────────────────────────────────

  #[test]
  fn lookup_fresh_returns_entry_within_ttl() {
    let mut cache = DiscoveryCache::new();
    cache.upsert("h".to_string(), BTreeMap::new());
    // 1-hour TTL on a just-written entry → fresh.
    assert!(cache.lookup_fresh("h", Duration::from_secs(3600)).is_some());
  }

  #[test]
  fn lookup_fresh_returns_none_for_stale_entry() {
    let mut cache = DiscoveryCache::new();
    cache.entries.insert(
      "h".to_string(),
      CacheEntry {
        tools_by_server: BTreeMap::new(),
        // Backdate by 25 hours — beyond the 24h default TTL.
        cached_at: Utc::now() - chrono::Duration::hours(25),
      },
    );
    assert!(cache.lookup_fresh("h", DEFAULT_TTL).is_none());
  }

  #[test]
  fn lookup_fresh_returns_none_for_unknown_hash() {
    let cache = DiscoveryCache::new();
    assert!(cache.lookup_fresh("never-stored", DEFAULT_TTL).is_none());
  }
}
