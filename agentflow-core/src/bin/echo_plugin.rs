//! Reference plugin: declares one node type `echo_uppercase` that takes a
//! `text: FlowValue::Json(string)` input and returns its uppercase form.
//!
//! Implemented as a plain stdio JSON-RPC loop using only `serde_json` so it
//! also serves as a template for plugin authors who want to write plugins in
//! other languages — the wire contract is exactly what is shown here.

use serde_json::{Value, json};
use std::io::{self, BufRead, Write};

const PLUGIN_NAME: &str = "agentflow-echo-plugin";
const PLUGIN_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> io::Result<()> {
  let stdin = io::stdin();
  let stdout = io::stdout();
  let mut stdout = stdout.lock();
  let mut stdin = stdin.lock();
  let mut line = String::new();

  loop {
    line.clear();
    let n = stdin.read_line(&mut line)?;
    if n == 0 {
      break; // EOF: host closed our stdin, exit cleanly.
    }
    if line.trim().is_empty() {
      continue;
    }

    let request: Value = match serde_json::from_str(&line) {
      Ok(v) => v,
      Err(e) => {
        eprintln!("{PLUGIN_NAME}: ignoring non-JSON line: {e}");
        continue;
      }
    };

    let id = request.get("id").and_then(Value::as_u64).unwrap_or(0);
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");

    let response = handle_method(id, method, request.get("params"));
    let mut serialized = serde_json::to_string(&response)?;
    serialized.push('\n');
    stdout.write_all(serialized.as_bytes())?;
    stdout.flush()?;

    if method == "plugin/shutdown" {
      break;
    }
  }
  Ok(())
}

fn handle_method(id: u64, method: &str, params: Option<&Value>) -> Value {
  match method {
    "plugin/initialize" => json!({
      "jsonrpc": "2.0",
      "id": id,
      "result": {
        "plugin_name": PLUGIN_NAME,
        "plugin_version": PLUGIN_VERSION,
        "nodes": [
          {
            "type": "echo_uppercase",
            "description": "Uppercase a string (FlowValue::Json) input."
          }
        ]
      }
    }),
    "node/execute" => execute_node(id, params),
    "plugin/shutdown" => json!({
      "jsonrpc": "2.0",
      "id": id,
      "result": {}
    }),
    _ => method_not_found(id, method),
  }
}

fn execute_node(id: u64, params: Option<&Value>) -> Value {
  let params = match params {
    Some(p) => p,
    None => return invalid_params(id, "node/execute requires params"),
  };
  let node_type = params
    .get("node_type")
    .and_then(Value::as_str)
    .unwrap_or("");
  if node_type != "echo_uppercase" {
    return method_not_found(id, &format!("node type '{node_type}'"));
  }
  let inputs = match params.get("inputs") {
    Some(v) => v,
    None => return invalid_params(id, "missing 'inputs' object"),
  };

  // FlowValue::Json(string) → { "type": "json", "value": "..." }
  let text = inputs
    .get("text")
    .and_then(|v| v.get("value"))
    .and_then(Value::as_str);
  let Some(text) = text else {
    return invalid_params(id, "input 'text' must be FlowValue::Json(string)");
  };

  json!({
    "jsonrpc": "2.0",
    "id": id,
    "result": {
      "outputs": {
        "text": { "type": "json", "value": text.to_uppercase() }
      }
    }
  })
}

fn method_not_found(id: u64, method: &str) -> Value {
  json!({
    "jsonrpc": "2.0",
    "id": id,
    "error": {
      "code": -32601,
      "message": format!("method not found: '{method}'")
    }
  })
}

fn invalid_params(id: u64, message: &str) -> Value {
  json!({
    "jsonrpc": "2.0",
    "id": id,
    "error": {
      "code": -32602,
      "message": message
    }
  })
}
