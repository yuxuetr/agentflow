//! `agentflow memory` subcommand surface (P10.7.1).
//!
//! Today only `prune` is wired — the trait surface for retention-based
//! pruning lives in `agentflow-memory::layer` (`PreferenceStore::
//! prune_older_than`, `EntityFactStore::prune_invalidated`). Session
//! and semantic stores have per-session `clear` instead of
//! retention-based prune; they're out of scope for this slice but
//! can join the surface once the trait gains a matching method.

pub mod prune;
