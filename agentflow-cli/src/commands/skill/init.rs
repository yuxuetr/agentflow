use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub async fn execute(
  skill_dir: String,
  name: Option<String>,
  description: Option<String>,
  force: bool,
) -> Result<()> {
  let dir = PathBuf::from(skill_dir);
  let skill_name = name
    .map(|name| slugify_skill_name(&name))
    .unwrap_or_else(|| infer_skill_name(&dir));
  let description =
    description.unwrap_or_else(|| format!("AgentFlow skill scaffold for {skill_name}."));

  if skill_name.is_empty() {
    bail!(
      "Could not infer a valid skill name; pass --name with lowercase letters, digits, or hyphens"
    );
  }
  if dir.exists() && !force && directory_has_entries(&dir)? {
    bail!(
      "Skill directory '{}' already exists and is not empty; pass --force to overwrite scaffold files",
      dir.display()
    );
  }

  fs::create_dir_all(dir.join("scripts"))
    .with_context(|| format!("Failed to create scripts directory in '{}'", dir.display()))?;
  fs::create_dir_all(dir.join("references")).with_context(|| {
    format!(
      "Failed to create references directory in '{}'",
      dir.display()
    )
  })?;
  fs::create_dir_all(dir.join("tests"))
    .with_context(|| format!("Failed to create tests directory in '{}'", dir.display()))?;

  write_scaffold_file(
    &dir.join("SKILL.md"),
    &skill_md(&skill_name, &description),
    force,
  )?;
  write_scaffold_file(&dir.join("README.md"), &readme(&skill_name), force)?;
  write_scaffold_file(&dir.join("scripts").join("hello.py"), HELLO_SCRIPT, force)?;
  write_scaffold_file(
    &dir.join("references").join("example.md"),
    &reference_doc(&skill_name),
    force,
  )?;
  write_scaffold_file(&dir.join("tests").join("smoke.sh"), &smoke_test(), force)?;
  write_scaffold_file(&dir.join("tests").join("README.md"), TEST_README, force)?;

  println!("Created skill scaffold at {}", dir.display());
  println!("  SKILL.md");
  println!("  README.md");
  println!("  scripts/hello.py");
  println!("  references/example.md");
  println!("  tests/smoke.sh");
  println!();
  println!("Next: agentflow skill validate {}", dir.display());

  Ok(())
}

fn write_scaffold_file(path: &Path, content: &str, force: bool) -> Result<()> {
  if path.exists() && !force {
    bail!(
      "Refusing to overwrite existing file '{}'; pass --force to overwrite scaffold files",
      path.display()
    );
  }
  fs::write(path, content).with_context(|| format!("Failed to write '{}'", path.display()))?;
  Ok(())
}

fn directory_has_entries(path: &Path) -> Result<bool> {
  Ok(path.read_dir()?.next().is_some())
}

fn infer_skill_name(dir: &Path) -> String {
  dir
    .file_name()
    .and_then(|name| name.to_str())
    .map(slugify_skill_name)
    .unwrap_or_default()
}

fn slugify_skill_name(value: &str) -> String {
  let mut slug = String::new();
  let mut previous_hyphen = false;

  for ch in value.chars() {
    if ch.is_ascii_alphanumeric() {
      slug.push(ch.to_ascii_lowercase());
      previous_hyphen = false;
    } else if !previous_hyphen && !slug.is_empty() {
      slug.push('-');
      previous_hyphen = true;
    }
  }

  while slug.ends_with('-') {
    slug.pop();
  }
  slug.truncate(64);
  while slug.ends_with('-') {
    slug.pop();
  }
  slug
}

fn skill_md(name: &str, description: &str) -> String {
  format!(
    r#"---
name: {name}
description: {description}
metadata:
  version: "0.1.0"
allowed-tools: script
---

# {name}

Use this skill when the user asks for help with the task this skill packages.

## Workflow

1. Clarify the user's goal when the request is ambiguous.
2. Use bundled references before relying on general knowledge.
3. Use the script tool only for the scripts included with this skill.
4. Return concise results with any files or commands that matter.
"#
  )
}

fn readme(name: &str) -> String {
  format!(
    r#"# {name}

This directory is an AgentFlow skill scaffold.

## Files

- `SKILL.md`: portable skill manifest and instructions.
- `references/example.md`: bundled reference material loaded into the persona.
- `scripts/hello.py`: script tool example.
- `tests/smoke.sh`: local smoke-test skeleton.

## Validate

```sh
agentflow skill validate .
agentflow skill list-tools .
```
"#
  )
}

fn reference_doc(name: &str) -> String {
  format!(
    r#"# {name} Reference

Replace this file with domain-specific notes, policies, examples, or API details that the skill should always consider.
"#
  )
}

fn smoke_test() -> String {
  r#"#!/usr/bin/env sh
set -eu

agentflow skill validate "$(dirname "$0")/.."
agentflow skill list-tools "$(dirname "$0")/.."
"#
  .to_string()
}

const HELLO_SCRIPT: &str = r#"#!/usr/bin/env python3
import json
import sys


def main():
    payload = json.load(sys.stdin) if not sys.stdin.isatty() else {}
    name = payload.get("name", "AgentFlow")
    print(f"hello from {name}")


if __name__ == "__main__":
    main()
"#;

const TEST_README: &str = r#"# Skill Tests

Add regression prompts, expected tool calls, and fixture data here.

Run the smoke test from the skill directory:

```sh
sh tests/smoke.sh
```
"#;

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn slugifies_skill_names_for_skill_md() {
    assert_eq!(slugify_skill_name("My Skill!"), "my-skill");
    assert_eq!(slugify_skill_name("--Rust__Expert--"), "rust-expert");
    assert_eq!(slugify_skill_name("abc---123"), "abc-123");
  }
}
