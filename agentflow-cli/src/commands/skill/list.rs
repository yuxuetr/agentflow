use anyhow::Result;
use std::path::PathBuf;

use agentflow_skills::SkillLoader;

pub async fn execute(skills_dir: Option<String>) -> Result<()> {
    let dir = resolve_skills_dir(skills_dir);

    if !dir.exists() {
        println!("📂 Skills directory not found: {}", dir.display());
        println!("   Create a skill with: mkdir -p {}/my_skill && touch {}/my_skill/skill.toml",
            dir.display(), dir.display());
        return Ok(());
    }

    println!("📂 Scanning skills in: {}\n", dir.display());

    let entries = std::fs::read_dir(&dir)
        .map_err(|e| anyhow::anyhow!("Cannot read skills dir: {}", e))?;

    let mut found = 0usize;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let toml_path = path.join("skill.toml");
        if !toml_path.exists() {
            continue;
        }

        match SkillLoader::load(&path) {
            Ok(manifest) => {
                let warnings = SkillLoader::validate(&manifest, &path)
                    .unwrap_or_else(|_| vec![]);

                let status = if warnings.is_empty() { "✅" } else { "⚠ " };
                println!(
                    "{} {} v{}",
                    status, manifest.skill.name, manifest.skill.version
                );
                println!("   {}", manifest.skill.description);
                println!("   Model: {} | Tools: {} | Knowledge: {} | Memory: {}",
                    manifest.model.resolved_model(),
                    manifest.tools.len(),
                    manifest.knowledge.len(),
                    manifest.memory.as_ref().map(|m| m.memory_type.as_str()).unwrap_or("session"),
                );
                println!("   Path: {}", path.display());
                if !warnings.is_empty() {
                    for w in &warnings {
                        println!("   ⚠  {}", w);
                    }
                }
                println!();
                found += 1;
            }
            Err(e) => {
                println!("❌ {} — {}", path.display(), e);
                println!();
            }
        }
    }

    if found == 0 {
        println!("No valid skills found.");
        println!("Run `agentflow skill validate <path>` to check a specific skill.");
    } else {
        println!("Found {} skill(s).", found);
    }

    Ok(())
}

fn resolve_skills_dir(arg: Option<String>) -> PathBuf {
    match arg {
        Some(d) => PathBuf::from(d),
        None => {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".agentflow")
                .join("skills")
        }
    }
}
