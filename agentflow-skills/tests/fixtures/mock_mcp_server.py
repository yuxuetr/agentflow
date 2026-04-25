#!/usr/bin/env python3
import json
import sys


def respond(message, result=None, error=None):
    if "id" not in message:
        return
    response = {"jsonrpc": "2.0", "id": message["id"]}
    if error is not None:
        response["error"] = error
    else:
        response["result"] = result
    print(json.dumps(response, separators=(",", ":")), flush=True)


def tools():
    schema = {
        "type": "object",
        "properties": {"message": {"type": "string"}},
        "required": ["message"],
        "additionalProperties": False,
    }
    return [
        {
            "name": "echo",
            "description": "Echo a message through the mock MCP server",
            "inputSchema": schema,
        },
        {
            "name": "tool_error",
            "description": "Return an MCP tool result marked as an error",
            "inputSchema": {"type": "object", "properties": {}},
        },
        {
            "name": "rpc_error",
            "description": "Return a JSON-RPC error for tools/call",
            "inputSchema": {"type": "object", "properties": {}},
        },
    ]


def main():
    for line in sys.stdin:
        if not line.strip():
            continue
        message = json.loads(line)
        method = message.get("method")

        if method == "initialize":
            respond(
                message,
                {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "agentflow-test-mcp", "version": "0.1.0"},
                },
            )
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            respond(message, {"tools": tools()})
        elif method == "tools/call":
            params = message.get("params", {})
            name = params.get("name")
            arguments = params.get("arguments", {})
            if name == "echo":
                respond(
                    message,
                    {
                        "content": [
                            {
                                "type": "text",
                                "text": "echo: " + str(arguments.get("message", "")),
                            },
                            {
                                "type": "resource",
                                "uri": "mock://echo",
                                "mimeType": "text/plain",
                                "text": "resource: " + json.dumps(arguments, sort_keys=True),
                            },
                        ]
                    },
                )
            elif name == "tool_error":
                respond(
                    message,
                    {
                        "content": [
                            {"type": "text", "text": "mock tool reported a domain error"}
                        ],
                        "isError": True,
                    },
                )
            elif name == "rpc_error":
                respond(
                    message,
                    error={
                        "code": -32001,
                        "message": "mock JSON-RPC tool failure",
                    },
                )
            else:
                respond(
                    message,
                    error={"code": -32602, "message": "unknown tool: " + str(name)},
                )
        else:
            respond(
                message,
                error={"code": -32601, "message": "unknown method: " + str(method)},
            )


if __name__ == "__main__":
    main()
