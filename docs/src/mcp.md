# MCP Server (Experimental)

fsPulse includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that allows AI agents to query scan data, analyze integrity issues, and explore filesystem history.

> **Experimental:** MCP support in fsPulse is experimental. You may experience connectivity issues depending on your client and connection method. See [Setup](mcp/setup.md) for details on known limitations.

The MCP endpoint is served at `/mcp` on the same port as the web UI. It uses the Streamable HTTP transport, compatible with Claude Desktop, Claude Code, and other MCP clients.

## What Can an Agent Do?

The agent has access to fsPulse's full data model — roots, scans, items, versions, and hashes — through 10 tools. It can:

- **Explore** — browse directory trees and search for files at any point in time
- **Query** — run structured queries with filtering, aggregation, and ordering across all domains
- **Analyze** — investigate integrity issues, track storage growth, and identify high-churn files
- **Report** — summarize activity over time periods, compare scans, and generate trend data

The most effective use is iterative: start with a broad question ("what changed this week?"), then drill into specific folders, files, or time ranges based on what the agent finds. See [Sample Prompts](mcp/prompts.md) for examples.

## Pagination

All tools return at most 200 rows per call. The agent handles pagination automatically — if results are truncated, it can request the next page. You don't need to think about pagination in your prompts; the agent manages it as needed.

## Contents

- [Setup](mcp/setup.md) — Enable MCP and configure your client
- [Sample Prompts](mcp/prompts.md) — Example prompts and multi-step investigation workflows
- [Tools](mcp/tools.md) — The 10 tools available to AI agents
