use anyhow::{bail, Context, Result};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::error_context::mcp_context;
use agentflow_skills::{SkillBuilder, SkillLoader};

pub async fn execute(skill_dir: String, dry_run: bool, smoke: bool) -> Result<()> {
  let dir = canonical_skill_dir(&skill_dir)?;
  println!("🧪 Testing skill at {}", dir.display());

  let manifest = SkillLoader::load(&dir)
    .with_context(|| format!("Failed to load skill from '{}'", skill_dir))?;

  let warnings =
    SkillLoader::validate(&manifest, &dir).with_context(|| "Skill manifest validation failed")?;
  println!("✅ manifest: valid");
  for warning in &warnings {
    println!("   ⚠  {}", warning);
  }

  let registry = SkillBuilder::build_registry(&manifest, &dir)
    .await
    .with_context(|| mcp_context("Tool discovery failed", &manifest))?;
  let mut tool_names = registry
    .list()
    .iter()
    .map(|tool| tool.name().to_string())
    .collect::<Vec<_>>();
  tool_names.sort();
  println!("✅ tools: discovered {}", tool_names.len());
  if tool_names.is_empty() {
    println!("   none");
  } else {
    println!("   {}", tool_names.join(", "));
  }

  if dry_run {
    println!("✅ dry-run: skipped regressions and smoke tests");
    println!("\n✅ Skill dry run passed");
    return Ok(());
  }

  let regression_count = run_minimal_regressions(&dir, &registry).await?;
  println!("✅ regressions: {} passed", regression_count);

  if smoke {
    run_smoke_script(&dir)?;
  }

  println!("\n✅ Skill test passed");
  Ok(())
}

fn canonical_skill_dir(skill_dir: &str) -> Result<PathBuf> {
  let dir = PathBuf::from(skill_dir);
  if dir.exists() {
    dir
      .canonicalize()
      .with_context(|| format!("Failed to canonicalize skill directory '{}'", dir.display()))
  } else {
    Ok(dir)
  }
}

async fn run_minimal_regressions(
  skill_dir: &Path,
  registry: &agentflow_tools::ToolRegistry,
) -> Result<usize> {
  let mut count = 0;

  if registry.get("script").is_some() {
    let hello_script = skill_dir.join("scripts").join("hello.py");
    if hello_script.is_file() {
      let output = registry
        .execute(
          "script",
          json!({"script": "hello.py", "args": {"name": "skill-test"}}),
        )
        .await
        .context("Minimal script tool regression failed")?;
      if output.is_error {
        bail!(
          "Minimal script tool regression returned an error: {}",
          output.content
        );
      }
      println!("   script hello.py: {}", output.content);
      count += 1;
    } else {
      println!("   script: skipped (scripts/hello.py not found)");
    }
  }

  Ok(count)
}

fn run_smoke_script(skill_dir: &Path) -> Result<()> {
  let smoke = skill_dir.join("tests").join("smoke.sh");
  if !smoke.is_file() {
    println!("✅ smoke: skipped (tests/smoke.sh not found)");
    return Ok(());
  }

  let status = Command::new("sh")
    .arg(&smoke)
    .current_dir(skill_dir)
    .status()
    .with_context(|| format!("Failed to run smoke test '{}'", smoke.display()))?;
  if !status.success() {
    bail!(
      "Smoke test '{}' failed with status {}",
      smoke.display(),
      status
    );
  }
  println!("✅ smoke: tests/smoke.sh passed");
  Ok(())
}
