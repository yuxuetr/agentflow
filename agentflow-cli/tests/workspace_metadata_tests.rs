use std::{fs, path::Path};

#[test]
fn workspace_members_use_rust_2024_edition() {
  let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
  let workspace_root = manifest_dir
    .parent()
    .expect("agentflow-cli should live under workspace root");
  let root_manifest = fs::read_to_string(workspace_root.join("Cargo.toml"))
    .expect("workspace Cargo.toml should be readable");
  let root_value: toml::Value =
    toml::from_str(&root_manifest).expect("workspace Cargo.toml should parse");
  let members = root_value["workspace"]["members"]
    .as_array()
    .expect("workspace members should be an array");

  assert!(!members.is_empty(), "workspace should declare members");

  // Resolve the inheritance target once. Every agentflow-* crate uses
  // `edition.workspace = true` to pull from [workspace.package]; only
  // xtask still spells it out literally.
  let workspace_edition = root_value
    .get("workspace")
    .and_then(|w| w.get("package"))
    .and_then(|p| p.get("edition"))
    .and_then(|e| e.as_str())
    .expect("[workspace.package].edition should be a string");

  for member in members {
    let member = member
      .as_str()
      .expect("workspace member entries should be strings");
    let member_manifest = fs::read_to_string(workspace_root.join(member).join("Cargo.toml"))
      .unwrap_or_else(|err| panic!("failed to read {member}/Cargo.toml: {err}"));
    let member_value: toml::Value =
      toml::from_str(&member_manifest).unwrap_or_else(|err| panic!("{member} should parse: {err}"));
    let edition_value = member_value["package"]
      .get("edition")
      .unwrap_or_else(|| panic!("{member} should declare package.edition"));

    let edition = if let Some(s) = edition_value.as_str() {
      s.to_string()
    } else if edition_value
      .as_table()
      .and_then(|t| t.get("workspace"))
      .and_then(|w| w.as_bool())
      .unwrap_or(false)
    {
      workspace_edition.to_string()
    } else {
      panic!("{member}/Cargo.toml package.edition must be a string or {{ workspace = true }}");
    };

    assert_eq!(edition, "2024", "{member} should use Rust 2024 edition");
  }
}
