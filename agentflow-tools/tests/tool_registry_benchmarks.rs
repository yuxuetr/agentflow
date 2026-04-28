//! ToolRegistry lookup, schema metadata, and execution wrapper benchmarks.
//!
//! Run with:
//!
//! ```bash
//! cargo test -p agentflow-tools --test tool_registry_benchmarks --target-dir /tmp/agentflow-target -- --nocapture
//! ```

use agentflow_tools::{Tool, ToolError, ToolMetadata, ToolOutput, ToolRegistry};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};

const ITERATIONS: usize = 10_000;

struct MockTool {
  name: String,
  fail: bool,
}

#[async_trait]
impl Tool for MockTool {
  fn name(&self) -> &str {
    &self.name
  }

  fn description(&self) -> &str {
    "mock benchmark tool"
  }

  fn parameters_schema(&self) -> Value {
    json!({
      "type": "object",
      "properties": {
        "input": {"type": "string"}
      },
      "required": ["input"]
    })
  }

  fn metadata(&self) -> ToolMetadata {
    ToolMetadata::builtin_named("mock")
  }

  async fn execute(&self, params: Value) -> Result<ToolOutput, ToolError> {
    if self.fail {
      Err(ToolError::ExecutionFailed {
        message: "mock failure".to_string(),
      })
    } else {
      Ok(ToolOutput::success(format!("ok:{}", params["input"])))
    }
  }
}

#[tokio::test]
async fn benchmark_tool_registry_lookup_metadata_and_execute() {
  println!("\nToolRegistry benchmarks");
  println!("{}", "=".repeat(80));

  let single = registry_with_tools(1, false);
  let medium = registry_with_tools(100, false);
  let large = registry_with_tools(10_000, false);
  let failing = registry_with_tools(1, true);

  let single_lookup = measure_sync("single tool lookup", ITERATIONS, || {
    single.get("mock_00000").expect("tool should exist");
  });
  let medium_lookup = measure_sync("100 tool lookup", ITERATIONS, || {
    medium.get("mock_00099").expect("tool should exist");
  });
  let large_lookup = measure_sync("10,000 tool lookup", ITERATIONS, || {
    large.get("mock_09999").expect("tool should exist");
  });

  let metadata = measure_sync("100 tool OpenAI schema metadata", 1_000, || {
    let tools = medium.openai_tools_array();
    assert_eq!(tools.len(), 100);
  });

  let success_execute = measure_async("successful execute wrapper", ITERATIONS, || {
    single.execute("mock_00000", json!({"input": "value"}))
  })
  .await;
  let error_execute = measure_async("error execute wrapper", ITERATIONS, || {
    failing.execute("mock_00000", json!({"input": "value"}))
  })
  .await;

  println!("\nSummary");
  println!("  single lookup avg: {:?}", single_lookup);
  println!("  100 lookup avg: {:?}", medium_lookup);
  println!("  10,000 lookup avg: {:?}", large_lookup);
  println!("  100 metadata avg: {:?}", metadata);
  println!("  success execute avg: {:?}", success_execute);
  println!("  error execute avg: {:?}", error_execute);
}

fn registry_with_tools(count: usize, fail: bool) -> ToolRegistry {
  let mut registry = ToolRegistry::new();
  for idx in 0..count {
    registry.register(Arc::new(MockTool {
      name: format!("mock_{idx:05}"),
      fail,
    }));
  }
  registry
}

fn measure_sync<F>(name: &str, iterations: usize, mut f: F) -> Duration
where
  F: FnMut(),
{
  let start = Instant::now();
  for _ in 0..iterations {
    f();
  }
  let total = start.elapsed();
  let avg = total / iterations as u32;
  println!("  {name} - avg: {avg:?} ({iterations} iterations, total: {total:?})");
  avg
}

async fn measure_async<F, Fut, T>(name: &str, iterations: usize, mut f: F) -> Duration
where
  F: FnMut() -> Fut,
  Fut: std::future::Future<Output = T>,
{
  let start = Instant::now();
  for _ in 0..iterations {
    let _ = f().await;
  }
  let total = start.elapsed();
  let avg = total / iterations as u32;
  println!("  {name} - avg: {avg:?} ({iterations} iterations, total: {total:?})");
  avg
}
