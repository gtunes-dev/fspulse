# Command-Line Interface

FsPulse provides multiple modes of operation:

- **Server mode** — Run as a background service with web UI (see `serve` command)
- **Command-line interface** (CLI) — Direct terminal commands documented on this page
- **Interactive modes** — Menu-driven and data explorer interfaces (see [Interactive Mode](interactive_mode.md))

This page documents the full CLI, including top-level commands, available subcommands, and commonly used options.

---

## Getting Help

At any time, you can get help from the CLI using:

```sh
fspulse --help
fspulse <command> --help
```

For example:

```sh
fspulse scan --help
fspulse report items --help
```

---

## Top-Level Commands

### `serve`

Start the FsPulse server to run as a background service with browser-based access.

```sh
fspulse serve
```

The server will start on the configured host and port (default: `http://127.0.0.1:8080`). Access the web UI to:
- Manage roots (create, view, delete)
- Initiate scans with real-time progress
- Query and explore data
- View scan results and changes

**Configuration**: Server host and port can be configured in `config.toml` under `[server]` section, or via environment variables `FSPULSE_SERVER_HOST` and `FSPULSE_SERVER_PORT`.

**Docker users**: The container automatically runs in serve mode. You can still access CLI commands via `docker exec`:
```sh
docker exec fspulse fspulse query "items limit 10"
docker exec -it fspulse fspulse interact
```

See the [Docker Deployment](docker.md) chapter for more details.

---

### `interact`

Launches FsPulse in interactive menu mode.

```sh
fspulse interact
```

- Provides a guided menu for scanning and reporting
- Allows interactive query mode with history
- Only usable for roots that have already been scanned

See the [Interactive Mode](interactive_mode.md) chapter for full details.

---

### `explore`

Launches an interactive, terminal-based data explorer.

```sh
fspulse explore
```

- Browse roots, scans, items, and changes in a full-screen TUI
- Navigate between entity views using keyboard shortcuts
- Filter and sort data interactively

This is different from `interact` (which is menu-driven). The `explore` command provides a more visual, spreadsheet-like interface for exploring your data.

---

### `scan`

Performs a filesystem scan.

```sh
fspulse scan [--root-id <id> | --root-path <path> | --last] [--hash] [--validate]
```

- `--root-id` — scan an existing root by ID
- `--root-path` — scan a new or existing root by its path
- `--last` — scan the most recently scanned root
- `--hash` — compute SHA2 hashes on new or changed files
- `--hash-all` — compute SHA2 hashes on all files (requires --hash)
- `--validate` — validate new or changed files with known formats (see [Validators](validators.md))
- `--validate-all` — validate all files (requires --validate)

### `report`

Generates prebuilt reports about roots, scans, items, or changes.

```sh
fspulse report <subcommand> [options]
```

Available subcommands:

#### `roots`
```sh
fspulse report roots [--root-id <id> | --root-path <path>]
```

#### `scans`
```sh
fspulse report scans [--scan-id <id> | --last <N>]
```

#### `items`
```sh
fspulse report items [--item-id <id> | --item-path <path> | --root-id <id>] [--invalid]
```

#### `changes`
```sh
fspulse report changes [--change-id <id> | --item-id <id> | --scan-id <id>]
```

Notes:
- `--invalid` on `items` requires `--root-id`

---

### `query`

Executes a structured query using FsPulse's flexible syntax.

```sh
fspulse query "<query string>"
```

Example:
```sh
fspulse query "items where item_path:('docs') order by mod_date desc limit 10"
```

See [Query Syntax](query.md) for full details.

---

## Database Location

FsPulse automatically determines the database location using the following precedence:

1. `FSPULSE_DATA_DIR` environment variable (if set)
2. `config.toml` `[database]` `path` setting (if configured)
3. Platform-specific data directory (default):
   - Linux: `~/.local/share/fspulse/`
   - macOS: `~/Library/Application Support/fspulse/`
   - Windows: `%LOCALAPPDATA%\fspulse\`

The database file is always named `fspulse.db` within the determined directory.

For Docker deployments, the database is stored in `/data/fspulse.db` inside the container. See the [Docker Deployment](docker.md) chapter for details.

For more information on configuration, see the [Configuration](configuration.md) chapter.

---

See also: [Interactive Mode](interactive_mode.md) · [Query Syntax](query.md) · [Configuration](configuration.md)

