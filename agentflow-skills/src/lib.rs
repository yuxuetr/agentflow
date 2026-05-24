//! # agentflow-skills
//!
//! Declarative skill system for AgentFlow.
//!
//! A **Skill** is preferably defined by a portable `SKILL.md` file with YAML
//! frontmatter and Markdown instructions. AgentFlow also supports `skill.toml`
//! as a structured compatibility manifest. When both files exist in one skill
//! directory, `skill.toml` is loaded as the active manifest.
//!
//! A `skill.toml` manifest can describe:
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
pub mod index;
pub mod loader;
pub mod manifest;
pub mod marketplace;
pub mod mcp_tools;
pub mod policy;
pub mod remote_marketplace;
pub mod skill_md;
pub mod validator;

pub use builder::SkillBuilder;
pub use error::SkillError;
pub use index::{
  DEFAULT_INDEX_FILE, ResolvedSkillRegistryEntry, SkillRegistryEntry, SkillRegistryIndex,
};
pub use loader::SkillLoader;
pub use manifest::{
  KnowledgeConfig, McpServerConfig, MemoryConfig, ModelConfig, PersonaConfig, SecurityConfig,
  SkillInfo, SkillManifest, ToolConfig, VALIDATOR_TIMEOUT_SECS_MAX, VALIDATOR_TIMEOUT_SECS_MIN,
  ValidationConfig,
};
pub use marketplace::{
  FeaturedSkill, MarketplaceResolvedSkill, MarketplaceSkillListing, SkillMarketplace,
  SkillMarketplaceIndex,
};
pub use mcp_tools::{McpClientPool, McpToolAdapter, public_tool_name};
pub use policy::{
  AdmissionSource, McpCapabilityMap, PolicyResolutionInput, ResolvedToolPolicy, ToolAdmission,
  resolve_tool_policy,
};
pub use remote_marketplace::{
  ArtifactFetchOutcome, CachedMarketplaceArtifact, ChecksumSha256SignatureVerifier,
  DEFAULT_MAX_ARTIFACT_BYTES, DEFAULT_MAX_MANIFEST_BYTES,
  DEFAULT_REMOTE_MARKETPLACE_SCHEMA_VERSION, Ed25519SignatureVerifier, MarketplacePackageType,
  MarketplaceSignature, MarketplaceSignatureVerifier, MarketplaceSource, RemoteMarketplaceCache,
  RemoteMarketplaceClient, RemoteMarketplaceEntry, RemoteMarketplaceManifest,
};
pub use skill_md::SkillMd;
pub use validator::{
  CommandValidator, RegexValidator, SkillValidator, VALIDATOR_UNRUNNABLE_EXIT_CODE,
  ValidatorVerdict, build_validator,
};
