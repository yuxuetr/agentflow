//! # agentflow-memory
//!
//! Memory backends for AgentFlow agents.
//!
//! Four layers (see `docs/MEMORY_LAYERING.md`):
//! - **Session** — `Message`-shaped rolling conversation. Implementations:
//!   [`SessionMemory`] (in-process), [`SqliteMemory`] (persistent),
//!   [`SemanticMemory`] (persistent + embeddings).
//! - **Semantic** — typed similarity search via [`SemanticMemoryStore`].
//! - **Preference** — durable per-user key/value. Implementation:
//!   [`SqlitePreferenceStore`].
//! - **Entity facts** — provenance-tracked structured facts.
//!   Implementation: [`SqliteEntityFactStore`].
//!
//! Stability: `MemoryStore` is stable. The four new types
//! (`MemoryLayer` / `PreferenceStore` / `EntityFactStore` /
//! `SemanticMemoryStore`) are experimental in this release — see
//! `docs/STABILITY.md` and the design doc for the promotion path.

pub mod entity_facts;
pub mod layer;
pub mod preference;
pub mod preference_encrypted;
pub mod semantic;
pub mod session;
pub mod sqlite;
mod sqlite_pool;

// The storage *contracts* (`MemoryError`, `Message`/`Role`/`TokenCounter`,
// `MemoryStore`) moved to `agentflow-store-spi` (P-A1.2). Re-export them under
// their original `agentflow_memory::{error,store,types}` module paths + crate
// root so every consumer — and this crate's own impls — keep compiling.
pub use agentflow_store_spi::{error, store, types};

pub use entity_facts::SqliteEntityFactStore;
pub use error::MemoryError;
pub use layer::{
  EntityFact, EntityFactStore, MemoryLayer, PreferenceScope, PreferenceStore, PreferenceValue,
  RetentionPolicy, SemanticMemoryStore,
};
pub use preference::SqlitePreferenceStore;
pub use preference_encrypted::{
  AgeEncryptedPreferenceStore, generate_identity_file, load_identity_file,
};
pub use semantic::SemanticMemory;
pub use session::SessionMemory;
pub use sqlite::SqliteMemory;
pub use store::MemoryStore;
pub use types::{HeuristicCounter, Message, Role, TokenCounter};
