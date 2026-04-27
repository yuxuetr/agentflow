//! Fixed DAG workflow example.
//!
//! This example demonstrates a deterministic workflow with explicit node
//! dependencies and input mappings. It does not use an LLM or agent runtime.
//!
//! Run:
//! ```sh
//! cargo run -p agentflow-core --example fixed_dag_workflow
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use agentflow_core::{
  async_node::{AsyncNode, AsyncNodeInputs, AsyncNodeResult},
  error::AgentFlowError,
  flow::{Flow, GraphNode, NodeType},
  value::FlowValue,
};
use async_trait::async_trait;
use serde_json::{json, Value};

struct ValidateOrderNode;
struct CalculateSubtotalNode;
struct CalculateShippingNode;
struct FinalizeInvoiceNode;

#[async_trait]
impl AsyncNode for ValidateOrderNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let order = json_input(inputs, "order")?;
    let customer_id = order
      .get("customer_id")
      .and_then(Value::as_str)
      .ok_or_else(|| input_error("order.customer_id must be a string"))?;
    let items = order
      .get("items")
      .and_then(Value::as_array)
      .ok_or_else(|| input_error("order.items must be an array"))?;

    if items.is_empty() {
      return Err(input_error("order.items must not be empty"));
    }

    let mut outputs = HashMap::new();
    outputs.insert(
      "validated_order".to_string(),
      FlowValue::Json(json!({
        "customer_id": customer_id,
        "items": items,
        "currency": order.get("currency").and_then(Value::as_str).unwrap_or("USD")
      })),
    );
    Ok(outputs)
  }
}

#[async_trait]
impl AsyncNode for CalculateSubtotalNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let order = json_input(inputs, "order")?;
    let items = order
      .get("items")
      .and_then(Value::as_array)
      .ok_or_else(|| input_error("order.items must be an array"))?;

    let mut subtotal_cents = 0_i64;
    for item in items {
      let quantity = item
        .get("quantity")
        .and_then(Value::as_i64)
        .ok_or_else(|| input_error("item.quantity must be an integer"))?;
      let unit_price_cents = item
        .get("unit_price_cents")
        .and_then(Value::as_i64)
        .ok_or_else(|| input_error("item.unit_price_cents must be an integer"))?;
      subtotal_cents += quantity * unit_price_cents;
    }

    let mut outputs = HashMap::new();
    outputs.insert(
      "subtotal".to_string(),
      FlowValue::Json(json!({ "subtotal_cents": subtotal_cents })),
    );
    Ok(outputs)
  }
}

#[async_trait]
impl AsyncNode for CalculateShippingNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let order = json_input(inputs, "order")?;
    let items = order
      .get("items")
      .and_then(Value::as_array)
      .ok_or_else(|| input_error("order.items must be an array"))?;
    let item_count = items.len() as i64;
    let shipping_cents = 500 + (item_count.saturating_sub(1) * 150);

    let mut outputs = HashMap::new();
    outputs.insert(
      "shipping".to_string(),
      FlowValue::Json(json!({ "shipping_cents": shipping_cents })),
    );
    Ok(outputs)
  }
}

#[async_trait]
impl AsyncNode for FinalizeInvoiceNode {
  async fn execute(&self, inputs: &AsyncNodeInputs) -> AsyncNodeResult {
    let order = json_input(inputs, "order")?;
    let subtotal = json_input(inputs, "subtotal")?;
    let shipping = json_input(inputs, "shipping")?;

    let subtotal_cents = subtotal
      .get("subtotal_cents")
      .and_then(Value::as_i64)
      .ok_or_else(|| input_error("subtotal.subtotal_cents must be an integer"))?;
    let shipping_cents = shipping
      .get("shipping_cents")
      .and_then(Value::as_i64)
      .ok_or_else(|| input_error("shipping.shipping_cents must be an integer"))?;
    let tax_cents = (subtotal_cents as f64 * 0.0825).round() as i64;
    let total_cents = subtotal_cents + shipping_cents + tax_cents;

    let mut outputs = HashMap::new();
    outputs.insert(
      "invoice".to_string(),
      FlowValue::Json(json!({
        "customer_id": order.get("customer_id"),
        "currency": order.get("currency"),
        "subtotal_cents": subtotal_cents,
        "shipping_cents": shipping_cents,
        "tax_cents": tax_cents,
        "total_cents": total_cents
      })),
    );
    Ok(outputs)
  }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  let order = json!({
    "customer_id": "cust_123",
    "currency": "USD",
    "items": [
      { "sku": "book", "quantity": 2, "unit_price_cents": 1500 },
      { "sku": "pen", "quantity": 3, "unit_price_cents": 250 }
    ]
  });

  let flow = Flow::new(vec![
    GraphNode {
      id: "validate_order".to_string(),
      node_type: NodeType::Standard(Arc::new(ValidateOrderNode)),
      dependencies: Vec::new(),
      input_mapping: None,
      run_if: None,
      initial_inputs: HashMap::from([("order".to_string(), FlowValue::Json(order))]),
    },
    GraphNode {
      id: "calculate_subtotal".to_string(),
      node_type: NodeType::Standard(Arc::new(CalculateSubtotalNode)),
      dependencies: vec!["validate_order".to_string()],
      input_mapping: Some(HashMap::from([(
        "order".to_string(),
        ("validate_order".to_string(), "validated_order".to_string()),
      )])),
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "calculate_shipping".to_string(),
      node_type: NodeType::Standard(Arc::new(CalculateShippingNode)),
      dependencies: vec!["validate_order".to_string()],
      input_mapping: Some(HashMap::from([(
        "order".to_string(),
        ("validate_order".to_string(), "validated_order".to_string()),
      )])),
      run_if: None,
      initial_inputs: HashMap::new(),
    },
    GraphNode {
      id: "finalize_invoice".to_string(),
      node_type: NodeType::Standard(Arc::new(FinalizeInvoiceNode)),
      dependencies: vec![
        "calculate_subtotal".to_string(),
        "calculate_shipping".to_string(),
      ],
      input_mapping: Some(HashMap::from([
        (
          "order".to_string(),
          ("validate_order".to_string(), "validated_order".to_string()),
        ),
        (
          "subtotal".to_string(),
          ("calculate_subtotal".to_string(), "subtotal".to_string()),
        ),
        (
          "shipping".to_string(),
          ("calculate_shipping".to_string(), "shipping".to_string()),
        ),
      ])),
      run_if: None,
      initial_inputs: HashMap::new(),
    },
  ]);

  let state = flow.run().await?;
  let invoice = state
    .get("finalize_invoice")
    .and_then(|result| result.as_ref().ok())
    .and_then(|outputs| outputs.get("invoice"))
    .ok_or_else(|| input_error("finalize_invoice.invoice was not produced"))?;

  println!("Final invoice:");
  if let FlowValue::Json(value) = invoice {
    println!("{}", serde_json::to_string_pretty(value)?);
  }

  Ok(())
}

fn json_input<'a>(inputs: &'a AsyncNodeInputs, key: &str) -> Result<&'a Value, AgentFlowError> {
  match inputs.get(key) {
    Some(FlowValue::Json(value)) => Ok(value),
    Some(_) => Err(input_error(format!("{key} must be a JSON value"))),
    None => Err(input_error(format!("{key} is required"))),
  }
}

fn input_error(message: impl Into<String>) -> AgentFlowError {
  AgentFlowError::NodeInputError {
    message: message.into(),
  }
}
