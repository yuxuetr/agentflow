use anyhow::{Context, Result};
use std::path::Path;

use agentflow_skills::SkillLoader;

pub async fn execute(skill_dir: String) -> Result<()> {
    let dir = Path::new(&skill_dir);

    // Detect which manifest format is present
    let format_tag = if dir.join("skill.toml").exists() {
        "skill.toml"
    } else if dir.join("SKILL.md").exists() {
        "SKILL.md"
    } else {
        "unknown"
    };

    println!("🔍 Validating skill at: {} [{}]", dir.display(), format_tag);

    let manifest = SkillLoader::load(dir)
        .with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

    println!(
        "📦 Skill:  {} v{}",
        manifest.skill.name, manifest.skill.version
    );
    println!("   📝 {}", manifest.skill.description);
    println!("🧠 Model:  {} (max {} iters, budget {} tokens)",
        manifest.model.resolved_model(),
        manifest.model.resolved_max_iterations(),
        manifest.model.resolved_budget_tokens(),
    );

    // Tools
    if manifest.tools.is_empty() {
        println!("🔧 Tools:  none");
    } else {
        println!("🔧 Tools ({}):", manifest.tools.len());
        for t in &manifest.tools {
            println!("   - {}", t.name);
            if !t.allowed_commands.is_empty() {
                println!("     allowed_commands: {:?}", t.allowed_commands);
            }
            if !t.allowed_paths.is_empty() {
                println!("     allowed_paths: {:?}", t.allowed_paths);
            }
            if !t.allowed_domains.is_empty() {
                println!("     allowed_domains: {:?}", t.allowed_domains);
            }
        }
    }

    // Knowledge files (skill.toml [[knowledge]] entries)
    if manifest.knowledge.is_empty() {
        println!("📚 Knowledge: none");
    } else {
        println!("📚 Knowledge ({}):", manifest.knowledge.len());
        for k in &manifest.knowledge {
            let label = k.description.as_deref().unwrap_or(&k.path);
            println!("   - {} ({})", label, k.path);
        }
    }

    // references/ directory (Agent Skills standard)
    let refs_dir = dir.join("references");
    if refs_dir.is_dir() {
        let count = std::fs::read_dir(&refs_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        println!("📚 references/: {} file(s)", count);
    }

    // scripts/ directory
    let scripts_dir = dir.join("scripts");
    if scripts_dir.is_dir() {
        let count = std::fs::read_dir(&scripts_dir)
            .map(|rd| rd.filter_map(|e| e.ok()).filter(|e| e.path().is_file()).count())
            .unwrap_or(0);
        let names: Vec<String> = {
            let mut v: Vec<String> = std::fs::read_dir(&scripts_dir)
                .map(|rd| {
                    rd.filter_map(|e| e.ok())
                        .filter_map(|e| {
                            if e.path().is_file() {
                                e.file_name().into_string().ok()
                            } else {
                                None
                            }
                        })
                        .collect()
                })
                .unwrap_or_default();
            v.sort();
            v
        };
        println!("📜 scripts/ ({} file(s)): {}", count, names.join(", "));
    }

    // Memory
    if let Some(mem) = &manifest.memory {
        println!(
            "🗄️  Memory: {} (window: {} tokens)",
            mem.memory_type,
            mem.resolved_window_tokens()
        );
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
