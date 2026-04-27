//! Skill calls MCP tool example.
//!
//! This example loads the `mcp-basic` Skill, builds its ToolRegistry, and calls
//! a discovered MCP tool directly. It does not require an LLM provider.
//!
//! Run:
//! ```sh
//! cargo run -p agentflow-skills --example skill_calls_mcp_tool
//! ```

use std::path::Path;

use agentflow_skills::{SkillBuilder, SkillLoader};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let skill_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/skills/mcp-basic");
  let manifest = SkillLoader::load(&skill_dir)?;
  SkillLoader::validate(&manifest, &skill_dir)?;

  let registry = SkillBuilder::build_registry(&manifest, &skill_dir).await?;
  let tool_name = "mcp_local_demo_echo";
  let output = registry
    .execute(tool_name, json!({ "text": "from skill example" }))
    .await?;

  println!("Called tool: {tool_name}");
  println!("Output: {}", output.content);
  println!("Is error: {}", output.is_error);

  Ok(())
}
