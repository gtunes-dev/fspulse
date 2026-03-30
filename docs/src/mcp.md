# MCP Server (Experimental)

fsPulse includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that allows AI agents to query scan data, analyze integrity issues, and explore filesystem history.

> **Experimental:** MCP support in fsPulse is experimental. You may experience connectivity issues depending on your client and connection method. See [Setup](mcp/setup.md) for details on known limitations.

The MCP endpoint is served at `/mcp` on the same port as the web UI. It uses the Streamable HTTP transport, compatible with Claude Desktop, Claude Code, and other MCP clients.

- [Setup](mcp/setup.md) — Enable MCP and configure your client (three connection methods)
- [Sample Prompts](mcp/prompts.md) — Example prompts to try once connected
- [Tools](mcp/tools.md) — The 10 tools available to AI agents
