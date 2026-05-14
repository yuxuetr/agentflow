use agentflow_db::Run;
use agentflow_server::{
  ApiError, CancelRunResponse, CreateRunResponse, ListRunsResponse, RunGraphResponse, RunResponse,
  StreamedEvent,
};
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

const RUN_ID: &str = "11111111-1111-4111-8111-111111111111";

fn fixture_value(raw: &str) -> Value {
  serde_json::from_str(raw).expect("fixture should be valid JSON")
}

fn run(status: &str, finished_at: Option<&str>, error: Option<&str>) -> Run {
  Run {
    id: Uuid::parse_str(RUN_ID).unwrap(),
    workflow: "name: compat\nnodes: []\n".to_string(),
    status: status.to_string(),
    started_at: DateTime::parse_from_rfc3339("2026-05-10T00:00:00Z")
      .unwrap()
      .with_timezone(&Utc),
    finished_at: finished_at.map(|ts| {
      DateTime::parse_from_rfc3339(ts)
        .unwrap()
        .with_timezone(&Utc)
    }),
    run_dir: Some(format!("/tmp/agentflow/runs/{RUN_ID}")),
    tenant_id: "default".to_string(),
    error: error.map(ToString::to_string),
  }
}

#[test]
fn create_run_response_fixture_matches_wire_shape() {
  let response = CreateRunResponse {
    run_id: Uuid::parse_str(RUN_ID).unwrap(),
    status: "queued",
  };

  assert_eq!(
    serde_json::to_value(response).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/create_run_response.json"
    ))
  );
}

#[test]
fn get_run_response_fixture_matches_wire_shape() {
  let response = RunResponse {
    run: run("running", None, None),
  };

  assert_eq!(
    serde_json::to_value(response).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/get_run_response.json"
    ))
  );
}

#[test]
fn list_runs_response_fixture_matches_wire_shape() {
  let response = ListRunsResponse {
    runs: vec![run("running", None, None)],
  };

  assert_eq!(
    serde_json::to_value(response).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/list_runs_response.json"
    ))
  );
}

#[test]
fn cancel_run_response_fixture_matches_wire_shape() {
  let response = CancelRunResponse {
    run: run(
      "cancelled",
      Some("2026-05-10T00:00:05Z"),
      Some("cancel requested"),
    ),
    cancelled: true,
  };

  assert_eq!(
    serde_json::to_value(response).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/cancel_run_response.json"
    ))
  );
}

#[test]
fn run_graph_response_fixture_matches_wire_shape() {
  let response = RunGraphResponse {
    graph: json!({
      "nodes": [
        {
          "id": "render",
          "label": "render",
          "type": "template",
          "status": "completed"
        }
      ],
      "edges": []
    }),
    mermaid: "graph TD\n  render[render]\n".to_string(),
    active_node: Some("render".to_string()),
  };

  assert_eq!(
    serde_json::to_value(response).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/run_graph_response.json"
    ))
  );
}

#[test]
fn events_history_fixture_matches_streamed_event_shape_and_reconnect_filter() {
  let fixture = fixture_value(include_str!(
    "fixtures/rest_envelopes/events_history_response.json"
  ));
  let events: Vec<StreamedEvent> = serde_json::from_value(fixture.clone()).unwrap();

  assert_eq!(serde_json::to_value(&events).unwrap(), fixture);
  assert!(events.windows(2).all(|pair| pair[0].seq < pair[1].seq));

  let after_seq = 0;
  let resumed: Vec<_> = events
    .iter()
    .filter(|event| event.seq > after_seq)
    .map(|event| event.seq)
    .collect();
  assert_eq!(resumed, vec![1, 2]);
}

#[tokio::test]
async fn api_error_fixture_matches_unified_error_envelope() {
  let response = ApiError::NotFound(format!("run {RUN_ID} not found")).into_response();
  let bytes = axum::body::to_bytes(response.into_body(), 4096)
    .await
    .unwrap();

  assert_eq!(
    serde_json::from_slice::<Value>(&bytes).unwrap(),
    fixture_value(include_str!(
      "fixtures/rest_envelopes/api_error_response.json"
    ))
  );
}

#[test]
fn sse_streamed_event_fixture_matches_wire_shape() {
  let fixture = fixture_value(include_str!("fixtures/sse_events/streamed_event.json"));
  let event: StreamedEvent = serde_json::from_value(fixture.clone()).unwrap();

  assert_eq!(event.seq, 1);
  assert_eq!(event.kind, "node.completed");
  assert_eq!(event.payload["node_id"], "render");
  assert_eq!(serde_json::to_value(event).unwrap(), fixture);
}
