//! `agentflow-store-spi` — the storage contracts of the AgentFlow kernel.
//!
//! This crate holds the *interfaces and data types* for conversation memory:
//! the [`MemoryStore`](store::MemoryStore) trait, the [`Message`](types::Message)
//! / [`Role`](types::Role) / [`TokenCounter`](types::TokenCounter) types, and the
//! shared [`MemoryError`](error::MemoryError). The concrete stores
//! (`SessionMemory`, `SqliteMemory`, `SemanticMemory`, …) remain in
//! `agentflow-memory`, which re-exports everything here under its original paths.
//!
//! Extracted from `agentflow-memory` in P-A1.2 so that runtime/agent contracts
//! (`agentflow-agent-spi`) can depend on `Message` without depending on the
//! `memory` implementation crate (RFC §4 store-spi). The `EmbeddingProvider`
//! contract (evaluation R6) is a follow-up: it needs the rag/memory error
//! surfaces unified first.

pub mod error;
pub mod store;
pub mod types;

pub use error::MemoryError;
pub use store::MemoryStore;
pub use types::{HeuristicCounter, Message, Role, TokenCounter};
