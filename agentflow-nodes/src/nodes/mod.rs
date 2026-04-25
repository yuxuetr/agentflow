//! Node implementations module

// Text-based AI model nodes
pub mod llm;

// Image AI model nodes
pub mod image_edit;
pub mod image_to_image;
pub mod image_understand;
pub mod text_to_image;

// Audio AI model nodes
pub mod asr;
pub mod tts;

// Utility nodes
#[cfg(feature = "http")]
pub mod http;

#[cfg(feature = "file")]
pub mod file;

#[cfg(feature = "template")]
pub mod template;

#[cfg(feature = "batch")]
pub mod batch;

#[cfg(feature = "conditional")]
pub mod conditional;

// Specialized content processing nodes
pub mod arxiv;
pub mod markmap;

// Integration nodes
#[cfg(feature = "mcp")]
pub mod mcp;

#[cfg(feature = "rag")]
pub mod rag;
