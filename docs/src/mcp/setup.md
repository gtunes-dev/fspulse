# Setup (Experimental)

> **Experimental:** MCP support in fsPulse is experimental. The connection methods described below each have known limitations that are under active investigation. You may need to restart fsPulse or your client if connectivity issues occur.

## Enable the MCP Server

The MCP server is disabled by default. Enable it in the fsPulse Settings page under **MCP Server (Experimental)**, or add the following to your `config.toml`:

```toml
[mcp]
enabled = true
```

Restart fsPulse after changing this setting. You should see `MCP server enabled at /mcp` in the startup output.

## Choosing a Connection Method

There are two ways to connect an AI client to fsPulse's MCP server:

| Method | Client | Requires Proxy | Multi-Chat | Setup Effort |
|--------|--------|---------------|------------|--------------|
| [Claude Desktop](#claude-desktop) | Claude Desktop | No | No | Low |
| [Claude Code](#claude-code) | Claude Code | No | Yes | Low |

**Multi-Chat** means multiple chat sessions can use the MCP server at the same time.

## Claude Desktop

Claude Desktop connects to fsPulse using the Developer settings JSON config with [mcp-remote](https://www.npmjs.com/package/mcp-remote) as a stdio-to-HTTP bridge. This requires Node.js (for `npx`) and no reverse proxy, but has a limitation: **only one chat session at a time can use the MCP server**. Starting a new chat will not have access to fsPulse's tools until you restart Claude Desktop.

### Prerequisites

- [Node.js](https://nodejs.org/) must be installed (provides `npx`)
- On macOS, Node.js must have **Local Network** access (check **System Settings > Privacy & Security > Local Network** if connecting to a fsPulse instance on another machine on your network)

### Configuration

Open Claude Desktop's configuration file by going to **Settings > Developer** (under "Desktop app") and clicking **Edit Config**. Add an entry under `mcpServers`:

```json
{
  "mcpServers": {
    "fspulse": {
      "command": "npx",
      "args": [
        "mcp-remote",
        "http://localhost:8080/mcp"
      ]
    }
  }
}
```

Replace `localhost:8080` with the hostname and port of your fsPulse instance if it is running on a different machine.

Restart Claude Desktop. fsPulse should appear as an available MCP server in the first chat session you open.

> **Why the single-chat limitation?** The Developer config only supports stdio-based MCP servers. The `mcp-remote` bridge runs as a single subprocess that holds one HTTP session with fsPulse. Only the chat session that first initializes this connection can use it.

## Claude Code

Claude Code supports Streamable HTTP natively, with no bridge or proxy required. Each conversation gets its own independent session. Add to your `.mcp.json`:

```json
{
  "mcpServers": {
    "fspulse": {
      "type": "streamable-http",
      "url": "http://localhost:8080/mcp"
    }
  }
}
```

## Multiple Instances

You can connect to multiple fsPulse instances by giving each a unique name:

### Claude Desktop

```json
{
  "mcpServers": {
    "fspulse-local": {
      "command": "npx",
      "args": ["mcp-remote", "http://localhost:8080/mcp"]
    },
    "fspulse-remote": {
      "command": "npx",
      "args": ["mcp-remote", "http://my-server:8080/mcp"]
    }
  }
}
```

### Claude Code

```json
{
  "mcpServers": {
    "fspulse-local": {
      "type": "streamable-http",
      "url": "http://localhost:8080/mcp"
    },
    "fspulse-remote": {
      "type": "streamable-http",
      "url": "http://my-server:8080/mcp"
    }
  }
}
```

Reference a specific instance by name in your prompts:

> Show me the integrity report on fspulse-remote
