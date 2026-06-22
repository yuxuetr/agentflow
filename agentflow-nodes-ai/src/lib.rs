//! `agentflow-nodes-ai` — capability-backed `AsyncNode` adapters.
//!
//! The capability tier of the node library (LLM / audio / image / RAG / MCP),
//! split out of `agentflow-nodes` in P-A (RFC_NODES_DECOMPOSITION). Tool-tier
//! nodes (`template` / `file` / `http` / `batch` / `conditional` / `arxiv` /
//! `markmap`) stay in `agentflow-nodes`, which carries no capability deps; this
//! crate is the adapter layer that depends on `agentflow-llm` / `agentflow-rag`
//! / `agentflow-mcp`.

pub mod nodes;

pub use nodes::asr::ASRNode;
pub use nodes::image_edit::ImageEditNode;
pub use nodes::image_to_image::ImageToImageNode;
pub use nodes::image_understand::ImageUnderstandNode;
pub use nodes::llm::LlmNode;
pub use nodes::text_to_image::TextToImageNode;
pub use nodes::tts::TTSNode;

#[cfg(feature = "mcp")]
pub use nodes::mcp::MCPNode;
#[cfg(feature = "rag")]
pub use nodes::rag::RAGNode;
