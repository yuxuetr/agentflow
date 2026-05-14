//! Default [`ContextProvider`] implementations.
//!
//! Phase H1 ships four file-backed providers that read workspace
//! conventions used across AgentFlow projects:
//!
//! - [`AgentsMdProvider`] — workspace instructions in `AGENTS.md`.
//! - [`TodosMdProvider`] — short execution queue in `TODOs.md`.
//! - [`RoadmapMdProvider`] — direction section in `RoadMap.md`.
//! - [`WorkspaceLayoutProvider`] — top-level workspace listing.
//!
//! All providers are deterministic for a given filesystem state. None
//! of them traverse symlinks outside [`crate::HarnessContext::workspace_root`],
//! and none of them dump file content beyond declared character caps
//! (HARNESS_MODE_EVOLUTION Risk 4 — context overload).

use std::path::Path;

use async_trait::async_trait;
use tokio::fs;

use crate::context::{ContextItem, ContextPriority, ContextProvider, HarnessContext};
use crate::error::HarnessError;

/// Default character cap when reading a workspace doc file. Keeps the
/// provider from dumping arbitrarily large content into the prompt.
pub const DEFAULT_DOC_CHAR_CAP: usize = 8_000;

/// Approximate ratio used to translate characters into a token-cost
/// estimate. Errs on the high side intentionally so providers do not
/// under-report when the prompt assembler budgets context.
const CHARS_PER_TOKEN: usize = 4;

fn estimate_tokens(chars: usize) -> usize {
  chars.div_ceil(CHARS_PER_TOKEN).max(1)
}

async fn read_capped(path: &Path, cap: usize) -> Result<Option<String>, std::io::Error> {
  match fs::read_to_string(path).await {
    Ok(text) => {
      if text.chars().count() <= cap {
        Ok(Some(text))
      } else {
        let truncated: String = text.chars().take(cap).collect();
        Ok(Some(format!("{truncated}\n\n[...truncated]")))
      }
    }
    Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
    Err(err) => Err(err),
  }
}

/// Reads `<workspace_root>/AGENTS.md` and surfaces it as a high-priority
/// context item. Returns no items when the file is absent (no error).
#[derive(Debug, Default, Clone)]
pub struct AgentsMdProvider {
  cap: usize,
}

impl AgentsMdProvider {
  pub fn new() -> Self {
    Self {
      cap: DEFAULT_DOC_CHAR_CAP,
    }
  }

  pub fn with_char_cap(mut self, cap: usize) -> Self {
    self.cap = cap;
    self
  }
}

#[async_trait]
impl ContextProvider for AgentsMdProvider {
  fn name(&self) -> &str {
    "agents_md"
  }

  fn priority_hint(&self) -> ContextPriority {
    ContextPriority::Critical
  }

  async fn collect(&self, ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
    let path = ctx.workspace_root.join("AGENTS.md");
    let maybe = read_capped(&path, self.cap)
      .await
      .map_err(|err| HarnessError::context(self.name(), err.to_string()))?;
    Ok(
      maybe
        .into_iter()
        .map(|content| build_item(self.name(), ContextPriority::Critical, content, &path))
        .collect(),
    )
  }
}

/// Reads `<workspace_root>/TODOs.md` and surfaces it as a high-priority
/// context item. Returns no items when the file is absent.
#[derive(Debug, Default, Clone)]
pub struct TodosMdProvider {
  cap: usize,
}

impl TodosMdProvider {
  pub fn new() -> Self {
    Self {
      cap: DEFAULT_DOC_CHAR_CAP,
    }
  }

  pub fn with_char_cap(mut self, cap: usize) -> Self {
    self.cap = cap;
    self
  }
}

#[async_trait]
impl ContextProvider for TodosMdProvider {
  fn name(&self) -> &str {
    "todos_md"
  }

  fn priority_hint(&self) -> ContextPriority {
    ContextPriority::High
  }

  async fn collect(&self, ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
    let path = ctx.workspace_root.join("TODOs.md");
    let maybe = read_capped(&path, self.cap)
      .await
      .map_err(|err| HarnessError::context(self.name(), err.to_string()))?;
    Ok(
      maybe
        .into_iter()
        .map(|content| build_item(self.name(), ContextPriority::High, content, &path))
        .collect(),
    )
  }
}

/// Reads `<workspace_root>/RoadMap.md` and (when present) extracts the
/// `## Direction` section. Falls back to the leading slice up to
/// `cap` characters when no `Direction` header exists.
#[derive(Debug, Default, Clone)]
pub struct RoadmapMdProvider {
  cap: usize,
}

impl RoadmapMdProvider {
  pub fn new() -> Self {
    Self {
      cap: DEFAULT_DOC_CHAR_CAP,
    }
  }

  pub fn with_char_cap(mut self, cap: usize) -> Self {
    self.cap = cap;
    self
  }
}

#[async_trait]
impl ContextProvider for RoadmapMdProvider {
  fn name(&self) -> &str {
    "roadmap_md"
  }

  fn priority_hint(&self) -> ContextPriority {
    ContextPriority::Normal
  }

  async fn collect(&self, ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
    let path = ctx.workspace_root.join("RoadMap.md");
    let raw = match fs::read_to_string(&path).await {
      Ok(text) => text,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(err) => return Err(HarnessError::context(self.name(), err.to_string())),
    };
    let trimmed = extract_direction_section(&raw).unwrap_or(&raw);
    let content = if trimmed.chars().count() <= self.cap {
      trimmed.to_owned()
    } else {
      let prefix: String = trimmed.chars().take(self.cap).collect();
      format!("{prefix}\n\n[...truncated]")
    };
    Ok(vec![build_item(
      self.name(),
      ContextPriority::Normal,
      content,
      &path,
    )])
  }
}

/// Returns the slice between `## Direction` (case-sensitive) and the
/// next top-level `## ` header. Returns `None` when the header is
/// missing so the caller can fall back to a generic preview.
fn extract_direction_section(body: &str) -> Option<&str> {
  let start = body.find("\n## Direction")?;
  let after_header = &body[start + 1..];
  let next_header = after_header.find("\n## ").unwrap_or(after_header.len());
  Some(&after_header[..next_header])
}

/// Lists the top-level entries of the workspace root (one level deep).
/// Excludes dot-prefixed entries to avoid leaking `.git`, `.env`, etc.
#[derive(Debug, Default, Clone)]
pub struct WorkspaceLayoutProvider {
  max_entries: usize,
}

impl WorkspaceLayoutProvider {
  pub fn new() -> Self {
    Self { max_entries: 50 }
  }

  pub fn with_max_entries(mut self, max: usize) -> Self {
    self.max_entries = max;
    self
  }
}

#[async_trait]
impl ContextProvider for WorkspaceLayoutProvider {
  fn name(&self) -> &str {
    "workspace_layout"
  }

  fn priority_hint(&self) -> ContextPriority {
    ContextPriority::Low
  }

  async fn collect(&self, ctx: &HarnessContext) -> Result<Vec<ContextItem>, HarnessError> {
    let mut dir = match fs::read_dir(&ctx.workspace_root).await {
      Ok(rd) => rd,
      Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
      Err(err) => return Err(HarnessError::context(self.name(), err.to_string())),
    };
    let mut entries: Vec<(String, bool)> = Vec::new();
    while let Some(entry) = dir
      .next_entry()
      .await
      .map_err(|err| HarnessError::context(self.name(), err.to_string()))?
    {
      let name = entry.file_name().to_string_lossy().into_owned();
      if name.starts_with('.') {
        continue;
      }
      let is_dir = entry
        .file_type()
        .await
        .map(|ft| ft.is_dir())
        .unwrap_or(false);
      entries.push((name, is_dir));
      if entries.len() >= self.max_entries {
        break;
      }
    }
    if entries.is_empty() {
      return Ok(Vec::new());
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let mut body = String::from("Top-level workspace entries:\n");
    for (name, is_dir) in &entries {
      body.push_str(if *is_dir { "- 📁 " } else { "- 📄 " });
      body.push_str(name);
      body.push('\n');
    }
    Ok(vec![build_item(
      self.name(),
      ContextPriority::Low,
      body,
      &ctx.workspace_root,
    )])
  }
}

fn build_item(
  source: &str,
  priority: ContextPriority,
  content: String,
  path: &Path,
) -> ContextItem {
  let chars = content.chars().count();
  ContextItem {
    source: source.to_owned(),
    priority,
    token_estimate: estimate_tokens(chars),
    content,
    metadata: serde_json::json!({"path": path.to_string_lossy()}),
  }
}

/// Convenience: returns the four default providers as boxed trait
/// objects in priority order (Critical → Low). Phase H1 callers can
/// reuse this list directly; Phase H2+ may insert hook-driven providers
/// alongside.
pub fn default_providers() -> Vec<std::sync::Arc<dyn ContextProvider>> {
  use std::sync::Arc;
  vec![
    Arc::new(AgentsMdProvider::new()),
    Arc::new(TodosMdProvider::new()),
    Arc::new(RoadmapMdProvider::new()),
    Arc::new(WorkspaceLayoutProvider::new()),
  ]
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::context::{HarnessProfile, HarnessRuntimeKind};
  use std::path::PathBuf;
  use tempfile::TempDir;

  fn ctx(root: PathBuf) -> HarnessContext {
    HarnessContext {
      session_id: "sess-test".into(),
      workspace_root: root,
      user_input: "hello".into(),
      model: "mock".into(),
      runtime: HarnessRuntimeKind::React,
      profile: HarnessProfile::Local,
      metadata: serde_json::Value::Null,
    }
  }

  #[tokio::test]
  async fn agents_md_provider_emits_one_item_when_file_exists() {
    let dir = TempDir::new().unwrap();
    tokio::fs::write(dir.path().join("AGENTS.md"), "do not break the build\n")
      .await
      .unwrap();
    let provider = AgentsMdProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, "agents_md");
    assert_eq!(items[0].priority, ContextPriority::Critical);
    assert!(items[0].content.contains("do not break"));
    assert!(items[0].token_estimate >= 1);
  }

  #[tokio::test]
  async fn agents_md_provider_returns_empty_when_file_missing() {
    let dir = TempDir::new().unwrap();
    let provider = AgentsMdProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert!(items.is_empty());
  }

  #[tokio::test]
  async fn agents_md_provider_truncates_when_over_cap() {
    let dir = TempDir::new().unwrap();
    let big = "x".repeat(2_000);
    tokio::fs::write(dir.path().join("AGENTS.md"), &big)
      .await
      .unwrap();
    let provider = AgentsMdProvider::new().with_char_cap(100);
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].content.contains("[...truncated]"));
    assert!(items[0].content.chars().count() < 2_000);
  }

  #[tokio::test]
  async fn todos_md_provider_reads_file() {
    let dir = TempDir::new().unwrap();
    tokio::fs::write(dir.path().join("TODOs.md"), "- TODO sample task\n")
      .await
      .unwrap();
    let provider = TodosMdProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].source, "todos_md");
    assert_eq!(items[0].priority, ContextPriority::High);
    assert!(items[0].content.contains("sample task"));
  }

  #[tokio::test]
  async fn roadmap_provider_extracts_direction_section() {
    let dir = TempDir::new().unwrap();
    let body = "# Roadmap\n\n## Overview\n\nstuff\n\n## Direction\n\nfocus on harness mode\n\n## Later\n\nother things\n";
    tokio::fs::write(dir.path().join("RoadMap.md"), body)
      .await
      .unwrap();
    let provider = RoadmapMdProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].content.contains("focus on harness mode"));
    // Must not include the "## Later" section.
    assert!(!items[0].content.contains("other things"));
  }

  #[tokio::test]
  async fn roadmap_provider_falls_back_when_no_direction_section() {
    let dir = TempDir::new().unwrap();
    let body = "# Roadmap\n\nNo headings here.\n";
    tokio::fs::write(dir.path().join("RoadMap.md"), body)
      .await
      .unwrap();
    let provider = RoadmapMdProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].content.contains("No headings here"));
  }

  #[tokio::test]
  async fn workspace_layout_provider_lists_top_level_entries_excluding_dotfiles() {
    let dir = TempDir::new().unwrap();
    tokio::fs::create_dir(dir.path().join("src")).await.unwrap();
    tokio::fs::create_dir(dir.path().join(".git"))
      .await
      .unwrap();
    tokio::fs::write(dir.path().join("README.md"), "hi")
      .await
      .unwrap();
    let provider = WorkspaceLayoutProvider::new();
    let items = provider
      .collect(&ctx(dir.path().to_path_buf()))
      .await
      .unwrap();
    assert_eq!(items.len(), 1);
    let body = &items[0].content;
    assert!(body.contains("src"));
    assert!(body.contains("README.md"));
    assert!(!body.contains(".git"));
  }

  #[tokio::test]
  async fn workspace_layout_provider_returns_empty_on_missing_root() {
    let missing = PathBuf::from("/this/path/should/not/exist/agentflow-harness-test");
    let provider = WorkspaceLayoutProvider::new();
    let items = provider.collect(&ctx(missing)).await.unwrap();
    assert!(items.is_empty());
  }

  #[test]
  fn default_providers_returns_four_in_priority_order() {
    let providers = default_providers();
    let names: Vec<&str> = providers.iter().map(|p| p.name()).collect();
    assert_eq!(
      names,
      vec!["agents_md", "todos_md", "roadmap_md", "workspace_layout"]
    );
  }
}
