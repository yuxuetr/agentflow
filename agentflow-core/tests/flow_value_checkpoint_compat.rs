use std::collections::HashMap;
use std::path::PathBuf;

use agentflow_core::{
  checkpoint::{Checkpoint, CheckpointConfig, CheckpointManager},
  value::FlowValue,
};
use serde_json::json;

fn fixture_json(path: &str) -> serde_json::Value {
  serde_json::from_str(path).expect("fixture should be valid JSON")
}

#[test]
fn tagged_flow_value_fixtures_round_trip() {
  let cases = [
    (
      include_str!("fixtures/flow_value/tagged_json.json"),
      FlowValue::Json(json!({
        "answer": 42,
        "labels": ["stable", "checkpoint"]
      })),
    ),
    (
      include_str!("fixtures/flow_value/tagged_file.json"),
      FlowValue::File {
        path: PathBuf::from("/tmp/agentflow/answer.txt"),
        mime_type: Some("text/plain".to_string()),
      },
    ),
    (
      include_str!("fixtures/flow_value/tagged_url.json"),
      FlowValue::Url {
        url: "https://example.test/assets/answer.png".to_string(),
        mime_type: None,
      },
    ),
  ];

  for (fixture, expected) in cases {
    let encoded = fixture_json(fixture);
    let decoded: FlowValue = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(decoded, expected);
    assert_eq!(serde_json::to_value(decoded).unwrap(), encoded);
  }
}

#[test]
fn legacy_raw_json_checkpoint_values_read_as_json_flow_values() {
  let checkpoint: Checkpoint = serde_json::from_str(include_str!(
    "fixtures/checkpoints/legacy_raw_json_checkpoint.json"
  ))
  .unwrap();

  let node_state = checkpoint
    .state
    .get("legacy_node")
    .and_then(serde_json::Value::as_object)
    .expect("legacy checkpoint should contain object node outputs");

  let object_output: FlowValue =
    serde_json::from_value(node_state["object_output"].clone()).unwrap();
  assert_eq!(
    object_output,
    FlowValue::Json(json!({
      "answer": 42,
      "nested": {
        "ok": true
      }
    }))
  );

  let array_output: FlowValue = serde_json::from_value(node_state["array_output"].clone()).unwrap();
  assert_eq!(array_output, FlowValue::Json(json!(["alpha", "beta"])));

  let string_output: FlowValue =
    serde_json::from_value(node_state["string_output"].clone()).unwrap();
  assert_eq!(string_output, FlowValue::Json(json!("legacy text")));
}

#[tokio::test]
async fn checkpoint_writer_emits_tagged_flow_values() {
  let temp_dir = tempfile::tempdir().unwrap();
  let manager = CheckpointManager::new(
    CheckpointConfig::default()
      .with_checkpoint_dir(temp_dir.path())
      .with_auto_cleanup(false),
  )
  .unwrap();

  let mut node_outputs = HashMap::new();
  node_outputs.insert(
    "json_output".to_string(),
    serde_json::to_value(FlowValue::Json(json!({"ok": true}))).unwrap(),
  );
  node_outputs.insert(
    "file_output".to_string(),
    serde_json::to_value(FlowValue::File {
      path: PathBuf::from("/tmp/agentflow/report.md"),
      mime_type: Some("text/markdown".to_string()),
    })
    .unwrap(),
  );
  node_outputs.insert(
    "url_output".to_string(),
    serde_json::to_value(FlowValue::Url {
      url: "https://example.test/report".to_string(),
      mime_type: None,
    })
    .unwrap(),
  );

  let mut state = HashMap::new();
  state.insert(
    "node".to_string(),
    serde_json::to_value(node_outputs).unwrap(),
  );

  manager
    .save_checkpoint("tagged-writer-workflow", "node", &state)
    .await
    .unwrap();

  let latest = std::fs::read_to_string(
    temp_dir
      .path()
      .join("tagged-writer-workflow")
      .join("checkpoint_latest.json"),
  )
  .unwrap();
  let latest: serde_json::Value = serde_json::from_str(&latest).unwrap();
  let node = &latest["state"]["node"];

  assert_eq!(
    node["json_output"],
    json!({"type": "json", "value": {"ok": true}})
  );
  assert_eq!(
    node["file_output"],
    json!({
      "type": "file",
      "path": "/tmp/agentflow/report.md",
      "mime_type": "text/markdown"
    })
  );
  assert_eq!(
    node["url_output"],
    json!({
      "type": "url",
      "url": "https://example.test/report",
      "mime_type": null
    })
  );
}

/// Full public-API disk round-trip: write a checkpoint containing
/// `FlowValue::File` and `FlowValue::Url` outputs, reload it via
/// `CheckpointManager::load_latest_checkpoint`, and verify every
/// variant deserializes back to the **exact** original (not silently
/// collapsed to `FlowValue::Json`).
///
/// The on-disk encoding test above covers writer ⇒ JSON. The in-flow
/// `checkpoint_state_roundtrips_flowvalue_variants` unit test covers
/// the in-memory state-pool conversion. This test closes the gap —
/// `save_checkpoint` ⇒ atomic rename ⇒ `load_latest_checkpoint` ⇒
/// `serde_json::from_value::<FlowValue>` — and is the contract every
/// resume code path implicitly depends on.
#[tokio::test]
async fn flow_value_file_and_url_survive_disk_round_trip() {
  let temp_dir = tempfile::tempdir().unwrap();
  let manager = CheckpointManager::new(
    CheckpointConfig::default()
      .with_checkpoint_dir(temp_dir.path())
      .with_auto_cleanup(false),
  )
  .unwrap();

  let file_value = FlowValue::File {
    path: PathBuf::from("/tmp/agentflow/disk-roundtrip.pdf"),
    mime_type: Some("application/pdf".to_string()),
  };
  let url_value = FlowValue::Url {
    url: "https://example.test/manifest.json".to_string(),
    mime_type: None,
  };
  let json_value = FlowValue::Json(json!({"depth": 3, "items": [1, 2, 3]}));

  let mut node_outputs = HashMap::new();
  node_outputs.insert(
    "file_asset".to_string(),
    serde_json::to_value(&file_value).unwrap(),
  );
  node_outputs.insert(
    "asset_url".to_string(),
    serde_json::to_value(&url_value).unwrap(),
  );
  node_outputs.insert(
    "tagged_json".to_string(),
    serde_json::to_value(&json_value).unwrap(),
  );

  let mut state = HashMap::new();
  state.insert(
    "producer_node".to_string(),
    serde_json::to_value(node_outputs).unwrap(),
  );

  manager
    .save_checkpoint("disk-roundtrip-workflow", "producer_node", &state)
    .await
    .unwrap();

  // Reload via the public API.
  let loaded = manager
    .load_latest_checkpoint("disk-roundtrip-workflow")
    .await
    .unwrap()
    .expect("checkpoint must be visible after save");

  assert_eq!(loaded.workflow_id, "disk-roundtrip-workflow");
  assert_eq!(loaded.last_completed_node, "producer_node");

  let node = loaded
    .state
    .get("producer_node")
    .and_then(serde_json::Value::as_object)
    .expect("producer_node should be an object map after disk reload");

  let restored_file: FlowValue = serde_json::from_value(node["file_asset"].clone())
    .expect("FlowValue::File must deserialize from the persisted tag");
  let restored_url: FlowValue = serde_json::from_value(node["asset_url"].clone())
    .expect("FlowValue::Url must deserialize from the persisted tag");
  let restored_json: FlowValue = serde_json::from_value(node["tagged_json"].clone())
    .expect("FlowValue::Json must deserialize from the persisted tag");

  assert_eq!(
    restored_file, file_value,
    "FlowValue::File path + mime_type must survive disk round-trip with full type fidelity"
  );
  assert_eq!(
    restored_url, url_value,
    "FlowValue::Url url + mime_type must survive disk round-trip with full type fidelity"
  );
  assert_eq!(
    restored_json, json_value,
    "FlowValue::Json must round-trip without losing payload"
  );

  // Sanity: the variant must not silently collapse to FlowValue::Json
  // (that would defeat downstream code that pattern-matches on `File`
  // / `Url`).
  assert!(
    matches!(restored_file, FlowValue::File { .. }),
    "restored file output must remain FlowValue::File, got {:?}",
    restored_file
  );
  assert!(
    matches!(restored_url, FlowValue::Url { .. }),
    "restored url output must remain FlowValue::Url, got {:?}",
    restored_url
  );
}
