//! # agentflow-skills
//!
//! Declarative skill system for AgentFlow.
//!
//! A **Skill** is defined by a `skill.toml` manifest that describes:
//! - `[skill]` — name, version, description
//! - `[persona]` — LLM role / instruction text (becomes the system prompt)
//! - `[model]` — model name + runtime constraints (optional)
//! - `[[tools]]` — authorised tools with sandbox constraints (optional)
//! - `[[knowledge]]` — domain documents injected into context (optional)
//! - `[memory]` — memory backend: session, sqlite or none (optional)
//!
//! ## Quick start
//!
//! ```rust,ignore
//! use std::path::Path;
//! use agentflow_skills::{SkillLoader, SkillBuilder};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     agentflow_llm::AgentFlow::init().await?;
//!
//!     let dir = Path::new("./skills/rust_expert");
//!     let manifest = SkillLoader::load(dir)?;
//!     let warnings = SkillLoader::validate(&manifest, dir)?;
//!     for w in &warnings { eprintln!("⚠  {}", w); }
//!
//!     let mut agent = SkillBuilder::build(&manifest, dir).await?;
//!     let answer = agent.run("Review this code for safety issues").await?;
//!     println!("{}", answer);
//!     Ok(())
//! }
//! ```

pub mod builder;
pub mod error;
pub mod loader;
pub mod manifest;
pub mod skill_md;

pub use builder::SkillBuilder;
pub use error::SkillError;
pub use loader::SkillLoader;
pub use manifest::{
    KnowledgeConfig, MemoryConfig, ModelConfig, PersonaConfig, SkillInfo, SkillManifest,
    ToolConfig,
};
pub use skill_md::SkillMd;
