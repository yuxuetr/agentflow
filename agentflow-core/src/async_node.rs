use crate::{error::AgentFlowError, value::FlowValue};
use async_trait::async_trait;
use std::collections::HashMap;

/// The result type for asynchronous node execution.
pub type AsyncNodeResult = Result<HashMap<String, FlowValue>, AgentFlowError>;

/// The input type for asynchronous node execution.
pub type AsyncNodeInputs = HashMap<String, FlowValue>;

/// Defines the core behavior of an asynchronous node in a workflow.
///
/// This is the async version of the `Node` trait. It is intended for nodes that
/// perform I/O-bound operations, such as making network requests.
#[async_trait]
pub trait AsyncNode: Send + Sync {
    /// Executes the node's asynchronous logic.
    ///
    /// # Arguments
    ///
    /// * `inputs` - A map of input names to `FlowValue`s.
    ///
    /// # Returns
    ///
    /// An `AsyncNodeResult` which is a `Result` containing a map of output names to
    /// `FlowValue`s on success, or an `AgentFlowError` on failure.
    async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FlowValue;
    use serde_json::json;
    use tokio::time::{sleep, Duration};

    // A mock async node for testing.
    struct AsyncAdderNode {
        delay_ms: u64,
    }

    #[async_trait]
    impl AsyncNode for AsyncAdderNode {
        async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
            if self.delay_ms > 0 {
                sleep(Duration::from_millis(self.delay_ms)).await;
            }

            let a = match inputs.get("a").and_then(|v| match v {
                FlowValue::Json(val) => val.as_i64(),
                _ => None,
            }) {
                Some(val) => val,
                None => return Err(AgentFlowError::NodeInputError { message: "Input 'a' is missing or not an integer".to_string() }),
            };

            let b = match inputs.get("b").and_then(|v| match v {
                FlowValue::Json(val) => val.as_i64(),
                _ => None,
            }) {
                Some(val) => val,
                None => return Err(AgentFlowError::NodeInputError { message: "Input 'b' is missing or not an integer".to_string() }),
            };

            let result = a + b;
            let mut outputs = HashMap::new();
            outputs.insert("sum".to_string(), FlowValue::Json(json!(result)));

            Ok(outputs)
        }
    }

    #[tokio::test]
    async fn test_async_node_success() {
        let node = AsyncAdderNode { delay_ms: 10 };
        let mut inputs = HashMap::new();
        inputs.insert("a".to_string(), FlowValue::Json(json!(10)));
        inputs.insert("b".to_string(), FlowValue::Json(json!(5)));

        let result = node.execute(&inputs).await;
        assert!(result.is_ok());

        let outputs = result.unwrap();
        assert!(outputs.contains_key("sum"));

        match outputs.get("sum").unwrap() {
            FlowValue::Json(v) => assert_eq!(v.as_i64(), Some(15)),
            _ => panic!("Output 'sum' was not a FlowValue::Json"),
        }
    }

    #[tokio::test]
    async fn test_async_node_missing_input() {
        let node = AsyncAdderNode { delay_ms: 0 };
        let mut inputs = HashMap::new();
        inputs.insert("a".to_string(), FlowValue::Json(json!(10)));

        let result = node.execute(&inputs).await;
        assert!(result.is_err());

        match result.err().unwrap() {
            AgentFlowError::NodeInputError { message } => {
                assert_eq!(message, "Input 'b' is missing or not an integer");
            }
            _ => panic!("Wrong error type"),
        }
    }
}
