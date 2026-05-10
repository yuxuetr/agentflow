#!/usr/bin/env python3
import json
import sys


PLUGIN_NAME = "agentflow-echo-plugin"
PLUGIN_VERSION = "1.0.0"
NODES = [
  {
    "type": "echo",
    "description": "Return the supplied FlowValue inputs unchanged for plugin smoke tests.",
  }
]


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


def handle_execute(params):
  node_type = params.get("node_type")
  if node_type != "echo":
    raise ValueError(f"unsupported node type: {node_type}")
  return {
    "outputs": params.get("inputs", {}),
  }


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
