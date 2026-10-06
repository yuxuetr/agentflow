#!/usr/bin/env python3
import json
import sys


def send_response(request, result=None, error=None):
    if "id" not in request:
        return
    response = {"jsonrpc": "2.0", "id": request["id"]}
    if error is None:
        response["result"] = result
    else:
        response["error"] = error
    print(json.dumps(response, separators=(",", ":")), flush=True)


def tools():
    return [
        {
            "name": "echo",
            "description": "Echo text with a demo MCP prefix.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to echo.",
                    }
                },
                "required": ["text"],
                "additionalProperties": False,
            },
        },
        {
            "name": "status",
            "description": "Return the demo MCP server status.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "additionalProperties": False,
            },
        },
    ]


def handle_tool_call(request):
    params = request.get("params", {})
    name = params.get("name")
    arguments = params.get("arguments", {})

    if name == "echo":
        text = str(arguments.get("text", ""))
        send_response(
            request,
            {"content": [{"type": "text", "text": "mcp-basic: " + text}]},
        )
    elif name == "status":
        send_response(
            request,
            {
                "content": [
                    {
                        "type": "text",
                        "text": "local demo MCP server is ready",
                    }
                ]
            },
        )
    else:
        send_response(
            request,
            error={"code": -32602, "message": "unknown tool: " + str(name)},
        )


def main():
    for line in sys.stdin:
        if not line.strip():
            continue
        request = json.loads(line)
        method = request.get("method")

        if method == "initialize":
            send_response(
                request,
                {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "mcp-basic", "version": "1.0.0"},
                },
            )
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            send_response(request, {"tools": tools()})
        elif method == "tools/call":
            handle_tool_call(request)
        else:
            send_response(
                request,
                error={"code": -32601, "message": "unknown method: " + str(method)},
            )


if __name__ == "__main__":
    main()
