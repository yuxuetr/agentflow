//! Capability-backed `AsyncNode` adapters (LLM / audio / image / RAG / MCP).
//!
//! Split out of `agentflow-nodes` in P-A (RFC_NODES_DECOMPOSITION) so the
//! tool-tier `agentflow-nodes` crate carries no capability dependencies and
//! `agentflow-worker` can compile the tool nodes without dragging in
//! `agentflow-llm` / `agentflow-rag` / `agentflow-mcp`.

pub mod llm;

pub mod image_edit;
pub mod image_to_image;
pub mod image_understand;
pub mod text_to_image;

pub mod asr;
pub mod tts;

#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "rag")]
pub mod rag;
