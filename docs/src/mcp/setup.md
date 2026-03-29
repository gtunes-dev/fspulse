# Setup

## Enable the MCP Server

The MCP server is disabled by default. Enable it in the fsPulse Settings page under **MCP Server**, or add the following to your `config.toml`:

```toml
[mcp]
enabled = true
```

Restart fsPulse after changing this setting. You should see `MCP server enabled at /mcp` in the startup output.

## Claude Desktop

Claude Desktop does not connect to HTTP-based MCP servers directly. Use [mcp-remote](https://www.npmjs.com/package/mcp-remote) as a bridge.

Open Claude Desktop's configuration file by going to **Settings → Developer** (under "Desktop app") and clicking **Edit Config**. Add an entry under `mcpServers`:

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

Restart Claude Desktop. fsPulse should appear as an available MCP server.

To connect to a remote fsPulse instance, replace `localhost:8080` with the appropriate host and port.

## Claude Code

Claude Code supports HTTP MCP servers natively. Add to your `.mcp.json`:

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

Reference a specific instance by name in your prompts:

> Show me the integrity report on fspulse-remote
