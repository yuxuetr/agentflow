use anyhow::Result;
use std::path::{Path, PathBuf};

use agentflow_skills::SkillLoader;

pub async fn execute(skills_dir: Option<String>) -> Result<()> {
  let dir = resolve_skills_dir(skills_dir);

  if !dir.exists() {
    println!("📂 Skills directory not found: {}", dir.display());
    println!(
      "   Create a skill with: mkdir -p {}/my_skill && touch {}/my_skill/skill.toml",
      dir.display(),
      dir.display()
    );
    return Ok(());
  }

  println!("📂 Scanning skills in: {}\n", dir.display());

  let entries =
    std::fs::read_dir(&dir).map_err(|e| anyhow::anyhow!("Cannot read skills dir: {}", e))?;

  let mut found = 0usize;

  for entry in entries.flatten() {
    let path = entry.path();
    if !path.is_dir() {
      continue;
    }

    // Accept directories that contain either skill.toml OR SKILL.md
    let has_toml = path.join("skill.toml").exists();
    let has_skill_md = path.join("SKILL.md").exists();
    if !has_toml && !has_skill_md {
      continue;
    }
    let format_tag = if has_toml { "skill.toml" } else { "SKILL.md" };

    match SkillLoader::load(&path) {
      Ok(manifest) => {
        let warnings = SkillLoader::validate(&manifest, &path).unwrap_or_default();
        let status = if warnings.is_empty() { "✅" } else { "⚠ " };

        println!(
          "{} {} v{} [{}]",
          status, manifest.skill.name, manifest.skill.version, format_tag
        );
        println!("   {}", manifest.skill.description);

        // Tools
        if manifest.tools.is_empty() {
          println!("   🔧 Tools: none");
        } else {
          let names: Vec<&str> = manifest.tools.iter().map(|t| t.name.as_str()).collect();
          println!("   🔧 Tools: {}", names.join(", "));
        }

        // Knowledge + directories
        let scripts_count = count_dir_files(&path.join("scripts"));
        let refs_count = count_dir_files(&path.join("references"));
        let know_count = manifest.knowledge.len();

        let mut extras = Vec::new();
        if know_count > 0 {
          extras.push(format!("knowledge:{}", know_count));
        }
        if scripts_count > 0 {
          extras.push(format!("scripts:{}", scripts_count));
        }
        if refs_count > 0 {
          extras.push(format!("references:{}", refs_count));
        }
        if extras.is_empty() {
          println!("   📚 Docs: none");
        } else {
          println!("   📚 Docs: {}", extras.join(" | "));
        }

        println!(
          "   🧠 Model: {} | 🗄️  Memory: {}",
          manifest.model.resolved_model(),
          manifest
            .memory
            .as_ref()
            .map(|m| m.memory_type.as_str())
            .unwrap_or("session"),
        );
        println!("   📁 {}", path.display());

        for w in &warnings {
          println!("   ⚠  {}", w);
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

/// Count the number of files directly inside a directory (non-recursive).
fn count_dir_files(dir: &Path) -> usize {
  if !dir.is_dir() {
    return 0;
  }
  std::fs::read_dir(dir)
    .map(|rd| {
      rd.filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .count()
    })
    .unwrap_or(0)
}

fn resolve_skills_dir(arg: Option<String>) -> PathBuf {
  match arg {
    Some(d) => PathBuf::from(d),
    None => dirs::home_dir()
      .unwrap_or_else(|| PathBuf::from("."))
      .join(".agentflow")
      .join("skills"),
  }
}
