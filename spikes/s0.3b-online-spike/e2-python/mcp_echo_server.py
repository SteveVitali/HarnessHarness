#!/usr/bin/env python3
"""S0.3b — local reference MCP `echo` server (throwaway). The fixed local reference peer that
M-S1-7 measures a round-trip against (l1-spike-spec s3.1 item 4), served over stdio via the
candidate's OFFICIAL MCP SDK. No network, no model spend."""
from mcp.server.mcpserver import MCPServer

app = MCPServer("s03b-echo")


@app.tool()
def echo(text: str) -> str:
    """Echo the input back unchanged."""
    return text


if __name__ == "__main__":
    app.run()
