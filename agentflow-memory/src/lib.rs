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
pub mod error;
pub mod layer;
pub mod preference;
pub mod semantic;
pub mod session;
pub mod sqlite;
pub mod store;
pub mod types;

pub use entity_facts::SqliteEntityFactStore;
pub use error::MemoryError;
pub use layer::{
  EntityFact, EntityFactStore, MemoryLayer, PreferenceScope, PreferenceStore, PreferenceValue,
  RetentionPolicy, SemanticMemoryStore,
};
pub use preference::SqlitePreferenceStore;
pub use semantic::SemanticMemory;
pub use session::SessionMemory;
pub use sqlite::SqliteMemory;
pub use store::MemoryStore;
pub use types::{Message, Role};
