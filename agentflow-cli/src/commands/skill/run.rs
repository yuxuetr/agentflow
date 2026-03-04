use anyhow::{Context, Result};
use std::path::Path;

use agentflow_llm::AgentFlow;
use agentflow_skills::{SkillBuilder, SkillLoader};

pub async fn execute(skill_dir: String, message: String, session_id: Option<String>) -> Result<()> {
    let dir = Path::new(&skill_dir);

    // Load + validate manifest
    let manifest = SkillLoader::load(dir)
        .with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

    let warnings = SkillLoader::validate(&manifest, dir)
        .with_context(|| "Skill validation failed")?;
    for w in &warnings {
        eprintln!("⚠  {}", w);
    }

    println!(
        "🚀 Running skill '{}' v{}",
        manifest.skill.name, manifest.skill.version
    );

    // Initialise AgentFlow (loads LLM provider config)
    AgentFlow::init()
        .await
        .context("Failed to initialise AgentFlow — is your API key configured?")?;

    // Build the agent from the skill manifest
    let mut agent = SkillBuilder::build(&manifest, dir)
        .await
        .context("Failed to build agent from skill manifest")?;

    // Optionally reuse an existing session
    if let Some(sid) = session_id {
        agent = agent.with_session_id(sid);
    }

    println!("📝 Session: {}", agent.session_id);
    println!("💬 User: {}\n", message);

    let start = std::time::Instant::now();
    let answer = agent.run(&message).await.context("Agent run failed")?;
    let elapsed = start.elapsed();

    println!("🤖 Agent: {}", answer);
    println!("\n⏱  Completed in {:.2?}", elapsed);

    Ok(())
}
