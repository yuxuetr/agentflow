use crate::{error::AgentFlowError, value::FlowValue};
use std::collections::HashMap;

/// The result type for node execution, representing a map of named outputs.
pub type NodeResult = Result<HashMap<String, FlowValue>, AgentFlowError>;

/// The input type for node execution, representing a map of named inputs.
pub type NodeInputs = HashMap<String, FlowValue>;

/// Defines the core behavior of a node in a workflow.
///
/// A `Node` is a self-contained unit of execution that takes a set of named inputs
/// and produces a set of named outputs. It is designed to be stateless; all required
/// data should be provided via the `inputs` map.
pub trait Node: Send + Sync {
    /// Executes the node's logic.
    ///
    /// # Arguments
    ///
    /// * `inputs` - A map of input names to `FlowValue`s. The node should expect all
    ///   its required inputs to be present in this map.
    ///
    /// # Returns
    ///
    /// A `NodeResult` which is a `Result` containing a map of output names to `FlowValue`s
    /// on success, or an `AgentFlowError` on failure.
    fn execute(&self, inputs: &NodeInputs) -> NodeResult;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::FlowValue;
    use serde_json::json;

    // A mock node for testing the new trait.
    struct AdderNode;

    impl Node for AdderNode {
        fn execute(&self, inputs: &NodeInputs) -> NodeResult {
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

    #[test]
    fn test_new_node_trait_success() {
        let node = AdderNode;
        let mut inputs = HashMap::new();
        inputs.insert("a".to_string(), FlowValue::Json(json!(10)));
        inputs.insert("b".to_string(), FlowValue::Json(json!(5)));

        let result = node.execute(&inputs);
        assert!(result.is_ok());

        let outputs = result.unwrap();
        assert!(outputs.contains_key("sum"));

        match outputs.get("sum").unwrap() {
            FlowValue::Json(v) => assert_eq!(v.as_i64(), Some(15)),
            _ => panic!("Output 'sum' was not a FlowValue::Json"),
        }
    }

    #[test]
    fn test_new_node_trait_missing_input() {
        let node = AdderNode;
        let mut inputs = HashMap::new();
        inputs.insert("a".to_string(), FlowValue::Json(json!(10)));

        let result = node.execute(&inputs);
        assert!(result.is_err());

        match result.err().unwrap() {
            AgentFlowError::NodeInputError { message } => {
                assert_eq!(message, "Input 'b' is missing or not an integer");
            }
            _ => panic!("Wrong error type"),
        }
    }
}
