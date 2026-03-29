# MCP Server

fsPulse includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that allows AI agents to query scan data, analyze integrity issues, and explore filesystem history.

The MCP endpoint is served at `/mcp` on the same port as the web UI. It uses the Streamable HTTP transport, compatible with Claude Desktop (via mcp-remote), Claude Code, and other MCP clients.

- [Setup](mcp/setup.md) — Enable MCP and configure Claude Desktop or Claude Code
- [Sample Prompts](mcp/prompts.md) — Example prompts to try once connected
- [Tools](mcp/tools.md) — The 10 tools available to AI agents
