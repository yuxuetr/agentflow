use anyhow::{Context, Result};
use std::path::Path;

use agentflow_skills::{SkillLoader};

pub async fn execute(skill_dir: String) -> Result<()> {
    let dir = Path::new(&skill_dir);

    println!("🔍 Validating skill at: {}", dir.display());

    let manifest = SkillLoader::load(dir)
        .with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

    println!(
        "📦 Skill: {} v{} — {}",
        manifest.skill.name, manifest.skill.version, manifest.skill.description
    );
    println!("🧠 Model: {}", manifest.model.resolved_model());
    println!("🔄 Max iterations: {}", manifest.model.resolved_max_iterations());

    if manifest.tools.is_empty() {
        println!("🔧 Tools: none");
    } else {
        println!("🔧 Tools ({}):", manifest.tools.len());
        for t in &manifest.tools {
            println!("   - {}", t.name);
            if !t.allowed_commands.is_empty() {
                println!("     commands: {:?}", t.allowed_commands);
            }
            if !t.allowed_paths.is_empty() {
                println!("     paths: {:?}", t.allowed_paths);
            }
            if !t.allowed_domains.is_empty() {
                println!("     domains: {:?}", t.allowed_domains);
            }
        }
    }

    if manifest.knowledge.is_empty() {
        println!("📚 Knowledge: none");
    } else {
        println!("📚 Knowledge ({}):", manifest.knowledge.len());
        for k in &manifest.knowledge {
            let label = k.description.as_deref().unwrap_or(&k.path);
            println!("   - {} ({})", label, k.path);
        }
    }

    if let Some(mem) = &manifest.memory {
        println!("🗄️  Memory: {} (window: {} tokens)", mem.memory_type, mem.resolved_window_tokens());
    } else {
        println!("🗄️  Memory: session (default)");
    }

    // Run full validation
    let warnings = SkillLoader::validate(&manifest, dir)
        .with_context(|| "Validation failed")?;

    if warnings.is_empty() {
        println!("\n✅ Skill is valid!");
    } else {
        println!("\n✅ Skill is valid with {} warning(s):", warnings.len());
        for w in &warnings {
            println!("   ⚠  {}", w);
        }
    }

    Ok(())
}
