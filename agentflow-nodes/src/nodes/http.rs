use std::collections::HashMap;
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  value::FlowValue,
};
use agentflow_tools::builtin::HttpTool;
use agentflow_tools::{SandboxPolicy, Tool};
use async_trait::async_trait;
use serde_json::{Value, json};

/// Workflow node that performs an HTTP request.
///
/// `HttpNode` delegates to [`agentflow_tools::builtin::HttpTool`] so the
/// workflow node surface inherits the same SSRF defenses (private-IP /
/// cloud-metadata / link-local blocking), redirect cap (max 10 hops with
/// per-hop re-validation), 30-second timeout, and host allowlist that
/// the agent tool already enforces (Q1.3.2 — closes agentflow-nodes.md
/// C2). The default policy is permissive on domain allow-listing but
/// strict on private/loopback IP classes; pin via [`HttpNode::with_policy`]
/// to lock down further.
#[derive(Clone)]
pub struct HttpNode {
  policy: Arc<SandboxPolicy>,
}

impl std::fmt::Debug for HttpNode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("HttpNode").finish_non_exhaustive()
  }
}

impl HttpNode {
  pub fn new(policy: Arc<SandboxPolicy>) -> Self {
    Self { policy }
  }

  pub fn with_policy(mut self, policy: Arc<SandboxPolicy>) -> Self {
    self.policy = policy;
    self
  }
}

impl Default for HttpNode {
  fn default() -> Self {
    // The default policy denies private IPs / cloud metadata / loopback
    // but allows arbitrary public domains (allowed_domains empty == permissive).
    Self {
      policy: Arc::new(SandboxPolicy::default()),
    }
  }
}

#[async_trait]
impl AsyncNode for HttpNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let url = get_string_input(inputs, "url")?;
    let method = get_optional_string_input(inputs, "method")?.unwrap_or("GET");
    let headers = get_optional_map_input(inputs, "headers")?;
    let body = get_optional_string_input(inputs, "body")?;

    // Build the params object expected by HttpTool. `usize::MAX` for
    // max_response_chars preserves the legacy node behavior of returning
    // the full response body; HttpTool's 8 KB default only matters for
    // its agent-tool usage where a long body is noisy in transcripts.
    let mut params = json!({
        "url": url,
        "method": method,
    });
    if let Some(b) = body {
      params["body"] = json!(b);
    }
    if let Some(h) = headers {
      params["headers"] = json!(h);
    }

    let tool = HttpTool::new(self.policy.clone())
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: format!("HttpNode failed to build HTTP client: {err}"),
      })?
      .with_max_response_chars(usize::MAX);

    let output = tool
      .execute(params)
      .await
      .map_err(|err| AgentFlowError::AsyncExecutionError {
        message: err.to_string(),
      })?;

    // HttpTool emits a single-text-part `ToolOutput`. We don't have the
    // HTTP status code at this layer because the tool flattens it into
    // the body for prompt-engineering ergonomics, so the node returns
    // `body` (with the legacy field name preserved) and signals success
    // via `output.is_error`.
    let mut outputs = HashMap::new();
    outputs.insert(
      "status".to_string(),
      FlowValue::Json(json!(if output.is_error { 500 } else { 200 })),
    );
    outputs.insert("body".to_string(), FlowValue::Json(json!(output.content)));
    Ok(outputs)
  }
}

fn get_string_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a str, AgentFlowError> {
  inputs
    .get(key)
    .and_then(|v| match v {
      FlowValue::Json(Value::String(s)) => Some(s.as_str()),
      _ => None,
    })
    .ok_or_else(|| AgentFlowError::NodeInputError {
      message: format!(
        "Required string input '{}' is missing or has wrong type",
        key
      ),
    })
}

fn get_optional_string_input<'a>(
  inputs: &'a AsyncNodeInputs,
  key: &str,
) -> Result<Option<&'a str>, AgentFlowError> {
  match inputs.get(key) {
    None => Ok(None),
    Some(v) => match v {
      FlowValue::Json(Value::String(s)) => Ok(Some(s.as_str())),
      _ => Err(AgentFlowError::NodeInputError {
        message: format!("Input '{}' has wrong type, expected a string", key),
      }),
    },
  }
}

fn get_optional_map_input(
  inputs: &AsyncNodeInputs,
  key: &str,
) -> Result<Option<HashMap<String, String>>, AgentFlowError> {
  match inputs.get(key) {
    None => Ok(None),
    Some(FlowValue::Json(Value::Object(map))) => {
      let mut result = HashMap::new();
      for (k, v) in map {
        if let Value::String(s) = v {
          result.insert(k.clone(), s.clone());
        }
      }
      Ok(Some(result))
    }
    _ => Err(AgentFlowError::NodeInputError {
      message: format!("Input '{}' has wrong type, expected a map", key),
    }),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Q1.3.2 regression: a freshly-constructed `HttpNode` must refuse
  /// SSRF attempts at the cloud-metadata IP without the operator
  /// having to wire any policy — the default policy denies the
  /// LinkLocal / CloudMetadata classes automatically.
  #[tokio::test]
  async fn default_policy_rejects_cloud_metadata_ssrf() {
    let node = HttpNode::default();
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert(
      "url".to_string(),
      FlowValue::Json(json!("http://169.254.169.254/latest/meta-data/")),
    );

    let err = node.execute(&inputs).await.unwrap_err();
    let message = err.to_string();
    assert!(
      message.contains("CloudMetadata")
        || message.contains("LinkLocal")
        || message.contains("Sandbox"),
      "expected SSRF rejection message, got: {message}"
    );
  }

  #[tokio::test]
  async fn default_policy_rejects_private_ip() {
    let node = HttpNode::default();
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert("url".to_string(), FlowValue::Json(json!("http://10.0.0.1")));

    let err = node.execute(&inputs).await.unwrap_err();
    assert!(err.to_string().contains("Private") || err.to_string().contains("Sandbox"));
  }

  #[tokio::test]
  async fn explicit_policy_allows_loopback_for_tests() {
    // Confirms the policy wiring carries through. We can't easily run a
    // mock HTTP server here without re-creating one, but proving the
    // URL passes the validator (instead of being denied) is enough for
    // the wiring regression; the actual GET happens in
    // `agentflow-tools::builtin::http::tests` which has full coverage.
    use std::net::TcpListener;
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);

    let policy = Arc::new(SandboxPolicy {
      allow_loopback_network_access: true,
      ..SandboxPolicy::default()
    });
    let node = HttpNode::new(policy);
    let mut inputs = AsyncNodeInputs::new();
    inputs.insert(
      "url".to_string(),
      FlowValue::Json(json!(format!("http://127.0.0.1:{port}"))),
    );
    // The server is gone so we expect a connection error, not a
    // sandbox denial — that proves the URL passed the validator.
    let err = node.execute(&inputs).await.unwrap_err();
    let message = err.to_string();
    assert!(
      !message.contains("Loopback"),
      "loopback should be allowed under explicit policy, but got: {message}"
    );
  }
}
