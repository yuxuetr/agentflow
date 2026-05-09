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

  for member in members {
    let member = member
      .as_str()
      .expect("workspace member entries should be strings");
    let member_manifest = fs::read_to_string(workspace_root.join(member).join("Cargo.toml"))
      .unwrap_or_else(|err| panic!("failed to read {member}/Cargo.toml: {err}"));
    let member_value: toml::Value =
      toml::from_str(&member_manifest).unwrap_or_else(|err| panic!("{member} should parse: {err}"));
    let edition = member_value["package"]["edition"]
      .as_str()
      .unwrap_or_else(|| panic!("{member} should declare package.edition"));

    assert_eq!(edition, "2024", "{member} should use Rust 2024 edition");
  }
}
