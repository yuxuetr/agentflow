use std::fs;

use agentflow_skills::{
  MarketplacePackageType, RemoteMarketplaceManifest, SkillLoader, SkillManifest, SkillMd,
};

#[test]
fn skill_md_fixture_ignores_unknown_frontmatter_and_builds_manifest() {
  let skill = SkillMd::parse(include_str!("fixtures/manifests/skill_md/SKILL.md")).unwrap();

  assert_eq!(skill.name, "compat-skill");
  assert_eq!(skill.allowed_tools, vec!["file", "http"]);
  assert_eq!(skill.metadata.get("future_metadata_key").map(String::as_str), Some("preserved"));
  assert_eq!(skill.mcp_servers.len(), 1);
  assert_eq!(skill.security.resolved_mcp_default_timeout_secs(), 12);

  let manifest = skill.into_manifest();
  assert_eq!(manifest.skill.version, "2.1.0");
  assert_eq!(manifest.persona.language.as_deref(), Some("en"));
  assert_eq!(manifest.tools.len(), 2);
  assert_eq!(manifest.mcp_servers[0].name, "demo");
}

#[test]
fn skill_toml_fixture_ignores_unknown_optional_fields() {
  let manifest: SkillManifest =
    toml::from_str(include_str!("fixtures/manifests/skill_toml/skill.toml")).unwrap();

  assert_eq!(manifest.skill.name, "compat-toml");
  assert_eq!(manifest.model.resolved_model(), "mock");
  assert_eq!(manifest.security.resolved_mcp_default_timeout_secs(), 9);
  assert_eq!(manifest.tools[0].allowed_paths, vec!["./fixtures"]);
  assert_eq!(manifest.mcp_servers[0].resolved_timeout_secs(), 30);
  assert_eq!(manifest.knowledge[0].path, "knowledge.md");
  assert_eq!(manifest.memory.as_ref().unwrap().resolved_window_tokens(), 2048);
}

#[test]
fn skill_loader_prefers_skill_toml_over_skill_md() {
  let dir = tempfile::tempdir().unwrap();
  fs::write(
    dir.path().join("SKILL.md"),
    include_str!("fixtures/manifests/skill_md/SKILL.md"),
  )
  .unwrap();
  fs::write(
    dir.path().join("skill.toml"),
    include_str!("fixtures/manifests/skill_toml/skill.toml"),
  )
  .unwrap();

  let manifest = SkillLoader::load(dir.path()).unwrap();
  assert_eq!(manifest.skill.name, "compat-toml");
}

#[test]
fn remote_marketplace_fixture_ignores_unknown_optional_fields() {
  let manifest =
    RemoteMarketplaceManifest::parse_toml(include_str!("fixtures/marketplace/remote_marketplace.toml"))
      .unwrap();

  assert_eq!(manifest.name, "compat-marketplace");
  assert_eq!(manifest.entries().len(), 2);
  assert_eq!(manifest.entries()[0].package_type, MarketplacePackageType::Skill);
  assert_eq!(manifest.entries()[0].aliases, vec!["compat"]);
  assert_eq!(manifest.entries()[1].package_type, MarketplacePackageType::Plugin);
}
