//! # agentflow-memory
//!
//! Conversation memory backends for AgentFlow agents.
//!
//! Provides two implementations:
//! - [`SessionMemory`] — in-process, token-windowed memory for the active session.
//! - [`SqliteMemory`] — persistent SQLite store for long-term history.
//!
//! Both implement the [`MemoryStore`] trait so they are interchangeable.

pub mod error;
pub mod session;
pub mod sqlite;
pub mod store;
pub mod types;

pub use error::MemoryError;
pub use session::SessionMemory;
pub use sqlite::SqliteMemory;
pub use store::MemoryStore;
pub use types::{Message, Role};
