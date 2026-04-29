use agentflow_skills::{MemoryConfig, SkillManifest};

pub fn apply_memory_override(manifest: &mut SkillManifest, memory: Option<&str>) {
  let Some(memory_type) = memory else {
    return;
  };
  manifest.memory = Some(MemoryConfig {
    memory_type: memory_type.to_string(),
    db_path: None,
    window_tokens: None,
    embedding_model: None,
  });
}

pub fn memory_label(manifest: &SkillManifest) -> &str {
  manifest
    .memory
    .as_ref()
    .map(|memory| memory.memory_type.as_str())
    .unwrap_or("session")
}
