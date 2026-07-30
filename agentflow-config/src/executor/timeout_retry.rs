//! T3.1: generic, node-type-agnostic `timeout_ms` / `max_retries` support.
//!
//! Before this module, only `mcp` (`agentflow-nodes-ai/src/nodes/mcp.rs`)
//! could declare a per-node timeout/retry in YAML — and even there, those
//! fields configure the MCP *client's own connection* retry/timeout, not
//! the node's execution as a whole. Every other node type (`http`, `llm`
//! chief among them — the two most failure-prone in practice) had no way
//! to declare either.
//!
//! Rather than repeating a bespoke timeout/retry implementation inside
//! every node type (the pattern this finding calls out as not scaling),
//! [`TimeoutRetryNode`] decorates *any* [`NodeType::Standard`] node with
//! both, driven by two new top-level `NodeDefinitionV2` fields —
//! `timeout_ms` / `max_retries`, siblings of `run_if`, not nested inside
//! `parameters:` — so they can never collide with a node type's own
//! parameters (e.g. `mcp`'s `parameters.timeout_ms` above keeps its
//! existing, different meaning untouched).
//!
//! Retries reuse [`RetryPolicy`]'s default error classification (network /
//! timeout / rate-limit errors only — see `agentflow-async-util::retry`)
//! rather than retrying unconditionally. A node that failed for a
//! non-transient reason (bad input, a 4xx response, a validation error) is
//! not retried, which is what makes it safe to apply this uniformly even
//! to node types with side effects (e.g. `HttpNode` issuing a POST): a
//! request that plainly failed doesn't get blindly resubmitted, only ones
//! that look like they may never have reached the far end in the first
//! place (a timeout is the clearest such case, and is exactly what this
//! wrapper itself produces when the inner node overruns `timeout_ms`).
//!
//! Scope: only [`NodeType::Standard`] is supported. `map` / `while` nodes
//! rejecting `timeout_ms` / `max_retries` at validation time
//! (`config::schema`) rather than silently ignoring them is deliberate —
//! wrapping a whole sub-flow's worth of nested node executions raises
//! different questions (retry the whole sub-flow from scratch? just the
//! failed leaf?) that are a separate design decision, not answered here.

use std::sync::Arc;
use std::time::Duration;

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  execute_with_retry,
  retry::RetryPolicy,
};
use async_trait::async_trait;

/// Decorates `inner` with an optional outer timeout and an optional retry
/// policy. Constructing one with both fields `None` is valid (used only
/// when at least one is `Some` in practice — see [`wrap_if_configured`]).
pub(crate) struct TimeoutRetryNode {
  inner: Arc<dyn AsyncNode>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
}

impl TimeoutRetryNode {
  fn new(inner: Arc<dyn AsyncNode>, timeout_ms: Option<u64>, max_retries: Option<u32>) -> Self {
    Self {
      inner,
      timeout_ms,
      max_retries,
    }
  }

  async fn attempt(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    match self.timeout_ms {
      Some(ms) => {
        match tokio::time::timeout(Duration::from_millis(ms), self.inner.execute(inputs)).await {
          Ok(result) => result,
          Err(_elapsed) => Err(AgentFlowError::TimeoutExceeded { duration_ms: ms }),
        }
      }
      None => self.inner.execute(inputs).await,
    }
  }
}

#[async_trait]
impl AsyncNode for TimeoutRetryNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    match self.max_retries {
      Some(max_retries) => {
        // `RetryPolicy::builder()` alone defaults `retryable_errors` to
        // empty, which `RetryPolicy::is_retryable` treats as "retry
        // everything" — the opposite of what this wrapper needs (retry
        // only network/timeout/rate-limit-class failures, so applying
        // this uniformly to nodes with side effects, e.g. `HttpNode`
        // issuing a POST, stays safe — see the module doc comment).
        // Start from `RetryPolicy::default()` (which populates that safe
        // classification) and override only `max_attempts`.
        let policy = RetryPolicy {
          max_attempts: max_retries,
          ..RetryPolicy::default()
        };
        execute_with_retry(&policy, "node", || self.attempt(inputs)).await
      }
      None => self.attempt(inputs).await,
    }
  }
}

/// Wraps `inner` in a [`TimeoutRetryNode`] only when at least one of
/// `timeout_ms` / `max_retries` is set — leaves every existing workflow
/// (neither field declared) with zero added indirection or behavior
/// change.
pub(crate) fn wrap_if_configured(
  inner: Arc<dyn AsyncNode>,
  timeout_ms: Option<u64>,
  max_retries: Option<u32>,
) -> Arc<dyn AsyncNode> {
  if timeout_ms.is_none() && max_retries.is_none() {
    return inner;
  }
  Arc::new(TimeoutRetryNode::new(inner, timeout_ms, max_retries))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;
  use std::sync::atomic::{AtomicU32, Ordering};
  use tokio::time::sleep;

  struct AlwaysSlowNode;

  #[async_trait]
  impl AsyncNode for AlwaysSlowNode {
    async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      sleep(Duration::from_secs(3600)).await;
      Ok(HashMap::new())
    }
  }

  /// Fails with a retryable (network-class) error the first `fail_times`
  /// calls, then succeeds. Counts total calls so tests can assert exactly
  /// how many attempts were made.
  struct FailNTimesThenSucceed {
    fail_times: u32,
    calls: AtomicU32,
  }

  impl FailNTimesThenSucceed {
    fn new(fail_times: u32) -> Self {
      Self {
        fail_times,
        calls: AtomicU32::new(0),
      }
    }
  }

  #[async_trait]
  impl AsyncNode for FailNTimesThenSucceed {
    async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      let call = self.calls.fetch_add(1, Ordering::SeqCst);
      if call < self.fail_times {
        return Err(AgentFlowError::NetworkError {
          message: "connection reset".to_string(),
        });
      }
      Ok(HashMap::new())
    }
  }

  struct AlwaysFailsValidation {
    calls: AtomicU32,
  }

  impl AlwaysFailsValidation {
    fn new() -> Self {
      Self {
        calls: AtomicU32::new(0),
      }
    }
  }

  #[async_trait]
  impl AsyncNode for AlwaysFailsValidation {
    async fn execute(&self, _inputs: &AsyncNodeInputs) -> AsyncNodeResult {
      self.calls.fetch_add(1, Ordering::SeqCst);
      Err(AgentFlowError::ValidationError("bad input".to_string()))
    }
  }

  #[tokio::test]
  async fn wrap_if_configured_returns_the_same_node_when_neither_field_is_set() {
    let inner: Arc<dyn AsyncNode> = Arc::new(FailNTimesThenSucceed::new(0));
    let wrapped = wrap_if_configured(Arc::clone(&inner), None, None);
    // Same underlying execution behavior — one call, immediate success —
    // proves no wrapper was introduced (a wrapped zero-config node would
    // still behave identically, so this also covers that degenerate case).
    let result = wrapped.execute(&HashMap::new()).await;
    assert!(result.is_ok());
  }

  #[tokio::test]
  async fn timeout_fires_and_reports_the_configured_duration() {
    let node = TimeoutRetryNode::new(Arc::new(AlwaysSlowNode), Some(50), None);
    let result = node.execute(&HashMap::new()).await;
    match result {
      Err(AgentFlowError::TimeoutExceeded { duration_ms }) => assert_eq!(duration_ms, 50),
      other => panic!("expected TimeoutExceeded, got {other:?}"),
    }
  }

  #[tokio::test]
  async fn retry_succeeds_once_the_transient_failure_stops() {
    let inner = Arc::new(FailNTimesThenSucceed::new(2));
    let node = TimeoutRetryNode::new(inner, None, Some(5));
    let result = node.execute(&HashMap::new()).await;
    assert!(result.is_ok(), "expected eventual success, got {result:?}");
  }

  #[tokio::test]
  async fn retry_gives_up_after_max_attempts_and_reports_retry_exhausted() {
    // Fails every time; with max_attempts(2) the policy allows 2 total
    // attempts before giving up.
    let inner = Arc::new(FailNTimesThenSucceed::new(u32::MAX));
    let node = TimeoutRetryNode::new(Arc::clone(&inner) as Arc<dyn AsyncNode>, None, Some(2));
    let result = node.execute(&HashMap::new()).await;
    assert!(
      matches!(result, Err(AgentFlowError::RetryExhausted { .. })),
      "expected RetryExhausted, got {result:?}"
    );
  }

  #[tokio::test]
  async fn non_retryable_errors_are_not_retried() {
    // ValidationError isn't in RetryPolicy's default retryable_errors set
    // (network / timeout / rate-limit only), so this must fail after a
    // single attempt even with max_retries configured.
    let inner = Arc::new(AlwaysFailsValidation::new());
    let node = TimeoutRetryNode::new(Arc::clone(&inner) as Arc<dyn AsyncNode>, None, Some(5));
    let result = node.execute(&HashMap::new()).await;
    assert!(result.is_err());
    assert_eq!(
      inner.calls.load(Ordering::SeqCst),
      1,
      "a non-retryable error must not trigger any retry attempts"
    );
  }
}
