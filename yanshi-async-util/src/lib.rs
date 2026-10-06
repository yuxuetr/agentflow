//! `yanshi-async-util` — reliability combinators for Yanshi.
//!
//! Houses the retry ([`retry`]), timeout ([`timeout`]), and limit-racing
//! ([`race`]) primitives in one place so the executor (`yanshi-core`) and
//! the agent loop (`yanshi-agents`) share a single implementation instead of
//! duplicating them (RFC §7 law 7). Extracted from `yanshi-core` in P-A1.4
//! (retry/timeout) and P-A3.2 (race); `yanshi-core` re-exports the modules
//! under their original paths.

// `retry`/`timeout` refer to `crate::error::{YanshiError, Result}`; surface
// the graph error module under that path so the moved code resolves unchanged.
pub use yanshi_graph::error;

pub mod race;
pub mod retry;
pub mod timeout;

pub use race::{RaceOutcome, race_with_limits};
