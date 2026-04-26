use std::fs;
use std::path::{Path, PathBuf};

use agentflow_skills::{SkillBuilder, SkillError, SkillLoader};
use agentflow_tools::ToolOutputPart;
use serde_json::json;
use tempfile::TempDir;

fn fixture_server() -> PathBuf {
  Path::new(env!("CARGO_MANIFEST_DIR"))
    .join("tests")
    .join("fixtures")
    .join("mock_mcp_server.py")
}

fn write_file(path: &Path, content: &str) {
  fs::write(path, content).expect("write test file");
}

fn toml_skill(name: &str, server_name: &str, command: &str, args: &[String]) -> String {
  let args = args
    .iter()
    .map(|arg| format!("{:?}", arg))
    .collect::<Vec<_>>()
    .join(", ");

  format!(
    r#"
[skill]
name = "{name}"
version = "0.1.0"
description = "MCP integration test skill"

[persona]
role = "Use the configured tools."

[[mcp_servers]]
name = "{server_name}"
command = "{command}"
args = [{args}]
"#
  )
}

fn toml_skill_with_mcp_config(name: &str, mcp_config: &str) -> String {
  format!(
    r#"
[skill]
name = "{name}"
version = "0.1.0"
description = "MCP integration test skill"

[persona]
role = "Use the configured tools."

{mcp_config}
"#
  )
}

#[tokio::test]
async fn skill_toml_loads_mcp_tools_and_calls_through_registry() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill("toml-mcp", "fixture", "python3", &[server]),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  SkillLoader::validate(&manifest, dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();

  let tool = registry.get("mcp_fixture_echo").expect("MCP echo tool");
  let definition = tool.definition();
  assert_eq!(
    definition.parameters["properties"]["message"]["type"],
    json!("string"),
    "MCP input schema should pass through to Tool metadata"
  );

  let output = registry
    .execute("mcp_fixture_echo", json!({"message": "hello"}))
    .await
    .unwrap();
  assert!(!output.is_error);
  assert!(output.content.contains("echo: hello"));
  assert!(output.content.contains(r#"resource: {"message": "hello"}"#));
  assert_eq!(
    output.parts,
    vec![
      ToolOutputPart::Text {
        text: "echo: hello".to_string()
      },
      ToolOutputPart::Resource {
        uri: "mock://echo".to_string(),
        mime_type: Some("text/plain".to_string()),
        text: Some(r#"resource: {"message": "hello"}"#.to_string()),
      }
    ]
  );
}

#[tokio::test]
async fn skill_md_metadata_loads_mcp_tools_and_calls_through_registry() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  let mcp_servers = json!([{
    "name": "md-fixture",
    "command": "python3",
    "args": [server],
  }])
  .to_string();

  write_file(
    &dir.path().join("SKILL.md"),
    &format!(
      r#"---
name: md-mcp
description: Test SKILL.md MCP metadata.
metadata:
  mcp_servers: '{}'
---

# MCP Skill

Use the mock MCP tool.
"#,
      mcp_servers.replace('\'', "''")
    ),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  SkillLoader::validate(&manifest, dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();

  let output = registry
    .execute("mcp_md_fixture_echo", json!({"message": "from-md"}))
    .await
    .unwrap();
  assert!(!output.is_error);
  assert!(output.content.contains("echo: from-md"));
}

#[tokio::test]
async fn mcp_server_start_failure_maps_to_skill_error() {
  let dir = TempDir::new().unwrap();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill(
      "bad-mcp",
      "missing",
      "agentflow-no-such-mcp-server-command",
      &[],
    ),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  let err = match SkillBuilder::build_registry(&manifest, dir.path()).await {
    Ok(_) => panic!("expected MCP startup to fail"),
    Err(err) => err,
  };

  match err {
    SkillError::McpError(message) => {
      assert!(message.contains("missing"));
      assert!(message.contains("Failed to connect MCP server"));
    }
    other => panic!("expected MCP error, got {other:?}"),
  }
}

#[tokio::test]
async fn duplicate_mcp_public_tool_names_are_rejected() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &format!(
      r#"
[skill]
name = "conflict-mcp"
version = "0.1.0"
description = "MCP conflict test"

[persona]
role = "Use the configured tools."

[[mcp_servers]]
name = "fixture-one"
command = "python3"
args = [{server:?}]

[[mcp_servers]]
name = "fixture_one"
command = "python3"
args = [{server:?}]
"#
    ),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  let err = match SkillBuilder::build_registry(&manifest, dir.path()).await {
    Ok(_) => panic!("expected duplicate MCP tool names to fail"),
    Err(err) => err,
  };

  match err {
    SkillError::ValidationError { message } => {
      assert!(message.contains("Duplicate tool name"));
      assert!(message.contains("mcp_fixture_one_echo"));
    }
    other => panic!("expected validation error, got {other:?}"),
  }
}

#[tokio::test]
async fn mcp_tool_result_error_maps_to_tool_output_error() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill("tool-error-mcp", "fixture", "python3", &[server]),
  );
  let manifest = SkillLoader::load(dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();

  let output = registry
    .execute("mcp_fixture_tool_error", json!({}))
    .await
    .unwrap();
  assert!(output.is_error);
  assert!(output.content.contains("mock tool reported a domain error"));
}

#[tokio::test]
async fn mcp_server_env_is_passed_to_process() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill_with_mcp_config(
      "env-mcp",
      &format!(
        r#"
[[mcp_servers]]
name = "fixture"
command = "python3"
args = [{server:?}]

[mcp_servers.env]
AGENTFLOW_MCP_TEST_VALUE = "from-env"
"#
      ),
    ),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();
  let output = registry
    .execute(
      "mcp_fixture_env_echo",
      json!({"name": "AGENTFLOW_MCP_TEST_VALUE"}),
    )
    .await
    .unwrap();

  assert!(!output.is_error);
  assert_eq!(output.content, "AGENTFLOW_MCP_TEST_VALUE=from-env");
}

#[tokio::test]
async fn mcp_tool_call_respects_configured_timeout() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill_with_mcp_config(
      "timeout-mcp",
      &format!(
        r#"
[[mcp_servers]]
name = "fixture"
command = "python3"
args = [{server:?}]
timeout_secs = 1
"#
      ),
    ),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();
  let err = registry
    .execute("mcp_fixture_slow", json!({"seconds": 3}))
    .await
    .unwrap_err();
  let message = err.to_string();

  assert!(message.contains("MCP server 'fixture' tool 'slow' timed out"));
}

#[tokio::test]
async fn mcp_image_content_is_preserved_as_typed_output_part() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill("image-mcp", "fixture", "python3", &[server]),
  );

  let manifest = SkillLoader::load(dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();
  let output = registry
    .execute("mcp_fixture_image", json!({}))
    .await
    .unwrap();

  assert!(!output.is_error);
  assert_eq!(output.content, "[image:image/png;4 bytes]");
  assert_eq!(
    output.parts,
    vec![ToolOutputPart::Image {
      data: "aW1n".to_string(),
      mime_type: "image/png".to_string(),
    }]
  );
}

#[tokio::test]
async fn mcp_json_rpc_call_error_maps_to_tool_error() {
  let dir = TempDir::new().unwrap();
  let server = fixture_server().to_string_lossy().into_owned();
  write_file(
    &dir.path().join("skill.toml"),
    &toml_skill("rpc-error-mcp", "fixture", "python3", &[server]),
  );
  let manifest = SkillLoader::load(dir.path()).unwrap();
  let registry = SkillBuilder::build_registry(&manifest, dir.path())
    .await
    .unwrap();

  let err = registry
    .execute("mcp_fixture_rpc_error", json!({}))
    .await
    .unwrap_err();
  let message = err.to_string();
  assert!(message.contains("MCP server 'fixture' tool 'rpc_error' failed"));
  assert!(message.contains("mock JSON-RPC tool failure"));
}
