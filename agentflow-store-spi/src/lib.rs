//! `agentflow-store-spi` — the storage contracts of the AgentFlow kernel.
//!
//! This crate holds the *interfaces and data types* for the kernel's two
//! storage axes:
//!
//! - **Conversation memory** — the [`MemoryStore`](store::MemoryStore) trait,
//!   the [`Message`](types::Message) / [`Role`](types::Role) /
//!   [`TokenCounter`](types::TokenCounter) types, and the shared
//!   [`MemoryError`](error::MemoryError). The concrete stores (`SessionMemory`,
//!   `SqliteMemory`, `SemanticMemory`, …) remain in `agentflow-memory`, which
//!   re-exports everything here under its original paths.
//! - **Knowledge retrieval** — the [`KnowledgeBackend`](knowledge::KnowledgeBackend)
//!   trait + [`KnowledgeChunk`](knowledge::KnowledgeChunk) /
//!   [`KnowledgeError`](knowledge::KnowledgeError) (RFC §9). The concrete
//!   backends (BM25 in-memory + vector store) live in `agentflow-rag`.
//!
//! Memory was extracted from `agentflow-memory` in P-A1.2 so that runtime/agent
//! contracts (`agentflow-agent-spi`) can depend on `Message` without depending
//! on the `memory` implementation crate (RFC §4 store-spi); the knowledge
//! contract (P-A4.1) follows the same pattern for `agentflow-skills` ⟷
//! `agentflow-rag`. The `EmbeddingProvider` contract (evaluation R6) is a
//! follow-up: it needs the rag/memory error surfaces unified first.

pub mod error;
pub mod knowledge;
pub mod store;
pub mod types;

pub use error::MemoryError;
pub use knowledge::{KnowledgeBackend, KnowledgeChunk, KnowledgeError};
pub use store::MemoryStore;
pub use types::{HeuristicCounter, Message, Role, TokenCounter};
