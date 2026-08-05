//! V1.5 integration test: `LLMClient` retries a transient provider
//! failure (429/5xx) with backoff, driven by the resolved model
//! registry's `defaults.max_retries` / `defaults.retry_delay_ms`.
//!
//! Exercises the full `AgentFlow::model(...).execute()` path — not just
//! the extracted retry helper (unit-tested directly in
//! `agentflow-llm/src/client/llm_client.rs`) — through
//! `ModelRegistry::global()` + the mock provider's env-var-driven error
//! queue (`AGENTFLOW_MOCK_ERROR_STATUS_CODES`), which is seeded at
//! provider-construction time inside `load_config_from_yaml`.
//!
//! This file intentionally contains a single test function that mutates
//! process-global state (the model registry singleton + an env var).
//! Cargo builds each file under `tests/` as its own process, so this is
//! isolated from every other integration test file; running the two
//! scenarios sequentially inside one test (rather than as two `#[test]`
//! functions) avoids any intra-file race on that same global state.

use agentflow_llm::{AgentFlow, ModelRegistry};

#[tokio::test]
async fn llm_client_retry_behavior_driven_by_registry_defaults() {
  let retryable_yaml = r#"
models:
  retry-test-transient:
    vendor: mock
    type: chat
defaults:
  max_retries: 2
  retry_delay_ms: 1
"#;

  // Phase 1: one queued 429 followed by a normal response, with
  // `max_retries: 2` configured — the client must retry once and
  // ultimately succeed.
  // SAFETY: test-only env var read once at provider-construction time
  // inside the `load_config_from_yaml` call immediately below; no other
  // test in this process touches this var (see module doc).
  unsafe {
    std::env::set_var("AGENTFLOW_MOCK_ERROR_STATUS_CODES", "[429]");
  }
  ModelRegistry::global()
    .load_config_from_yaml(retryable_yaml)
    .await
    .expect("load hermetic mock config with one queued 429");

  let result = AgentFlow::model("retry-test-transient")
    .prompt("hello")
    .execute()
    .await;

  assert!(
    result.is_ok(),
    "expected eventual success after retrying one transient 429, got {result:?}"
  );

  // Phase 2: a non-retryable 400 must propagate immediately even with
  // retries configured — reload the registry (fresh mock provider, so
  // the error queue is re-seeded) with a 400 this time.
  // SAFETY: see phase 1 — same single-threaded sequential test.
  unsafe {
    std::env::set_var("AGENTFLOW_MOCK_ERROR_STATUS_CODES", "[400]");
  }
  ModelRegistry::global()
    .load_config_from_yaml(retryable_yaml)
    .await
    .expect("reload hermetic mock config with one queued 400");

  let result = AgentFlow::model("retry-test-transient")
    .prompt("hello")
    .execute()
    .await;

  // SAFETY: see phase 1.
  unsafe {
    std::env::remove_var("AGENTFLOW_MOCK_ERROR_STATUS_CODES");
  }

  assert!(
    result.is_err(),
    "a non-retryable 400 must not be swallowed by a retry"
  );
}
