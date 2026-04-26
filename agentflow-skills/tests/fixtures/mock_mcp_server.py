#!/usr/bin/env python3
import json
import os
import sys
import time


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
            "name": "env_echo",
            "description": "Return an environment variable from the MCP server process",
            "inputSchema": {
                "type": "object",
                "properties": {"name": {"type": "string"}},
                "required": ["name"],
                "additionalProperties": False,
            },
        },
        {
            "name": "slow",
            "description": "Sleep before returning a response",
            "inputSchema": {
                "type": "object",
                "properties": {"seconds": {"type": "number"}},
                "required": ["seconds"],
                "additionalProperties": False,
            },
        },
        {
            "name": "image",
            "description": "Return a tiny image content part",
            "inputSchema": {"type": "object", "properties": {}},
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
            elif name == "env_echo":
                env_name = str(arguments.get("name", ""))
                respond(
                    message,
                    {
                        "content": [
                            {
                                "type": "text",
                                "text": env_name + "=" + os.environ.get(env_name, ""),
                            }
                        ]
                    },
                )
            elif name == "slow":
                time.sleep(float(arguments.get("seconds", 0)))
                respond(
                    message,
                    {"content": [{"type": "text", "text": "slow response"}]},
                )
            elif name == "image":
                respond(
                    message,
                    {
                        "content": [
                            {
                                "type": "image",
                                "data": "aW1n",
                                "mimeType": "image/png",
                            }
                        ]
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
