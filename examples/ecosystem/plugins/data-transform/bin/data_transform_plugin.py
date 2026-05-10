#!/usr/bin/env python3
import json
import sys


PLUGIN_NAME = "agentflow-data-transform-plugin"
PLUGIN_VERSION = "1.0.0"
NODES = [
  {
    "type": "json_pick",
    "description": "Pick one key from a JSON FlowValue object and return it as output.value.",
  },
  {
    "type": "json_merge",
    "description": "Merge JSON object FlowValues into one output.object value.",
  },
]


def flow_json(value):
  if isinstance(value, dict) and value.get("type") == "json":
    return value.get("value")
  return value


def as_flow_json(value):
  return {
    "type": "json",
    "value": value,
  }


def respond(request_id, result=None, error=None):
  message = {
    "jsonrpc": "2.0",
    "id": request_id,
  }
  if error is not None:
    message["error"] = error
  else:
    message["result"] = result
  print(json.dumps(message, separators=(",", ":")), flush=True)


def handle_json_pick(inputs):
  source = flow_json(inputs.get("object", {}))
  key = flow_json(inputs.get("key", ""))
  if not isinstance(source, dict):
    raise ValueError("json_pick input 'object' must be a JSON object")
  if not isinstance(key, str) or not key:
    raise ValueError("json_pick input 'key' must be a non-empty string")
  return {
    "outputs": {
      "value": as_flow_json(source.get(key)),
    },
  }


def handle_json_merge(inputs):
  merged = {}
  for name, flow_value in inputs.items():
    value = flow_json(flow_value)
    if isinstance(value, dict):
      merged.update(value)
    else:
      merged[name] = value
  return {
    "outputs": {
      "object": as_flow_json(merged),
    },
  }


def handle_execute(params):
  node_type = params.get("node_type")
  inputs = params.get("inputs") or {}
  if node_type == "json_pick":
    return handle_json_pick(inputs)
  if node_type == "json_merge":
    return handle_json_merge(inputs)
  raise ValueError(f"unsupported node type: {node_type}")


def handle_request(request):
  method = request.get("method")
  params = request.get("params") or {}
  if method == "plugin/initialize":
    return {
      "plugin_name": PLUGIN_NAME,
      "plugin_version": PLUGIN_VERSION,
      "nodes": NODES,
    }
  if method == "node/execute":
    return handle_execute(params)
  if method == "plugin/shutdown":
    return {}
  raise ValueError(f"unknown method: {method}")


def main():
  for line in sys.stdin:
    line = line.strip()
    if not line:
      continue
    request = json.loads(line)
    request_id = request.get("id")
    try:
      respond(request_id, result=handle_request(request))
    except Exception as exc:
      respond(
        request_id,
        error={
          "code": -32000,
          "message": str(exc),
        },
      )


if __name__ == "__main__":
  main()
