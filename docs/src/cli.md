# Command-Line Interface

FsPulse provides two primary modes of operation:

- A powerful **command-line interface** (CLI)
- An interactive, menu-driven interface described in the [Interactive Mode](interactive_mode.md) section

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

### `interact`
Launches FsPulse in interactive mode. 

```sh
fspulse interact [--db-path <path>]
```

- Allows users to choose scan or report actions through a guided menu
- Only usable for roots that have already been scanned

### `scan`
Performs a filesystem scan.

```sh
fspulse scan [--db-path <path>] [--root-id <id> | --root-path <path> | --last] [--hash] [--validate]
```

- `--root-id` — scan an existing root by ID
- `--root-path` — scan a new or existing root by its path
- `--last` — scan the most recently scanned root
- `--hash` — compute MD5 hashes on new or changed files
- `--hash-all` — compute MD5 hashes on all files (requires --hash)
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
fspulse report roots [--db-path <path>] [--root-id <id> | --root-path <path>]
```

#### `scans`
```sh
fspulse report scans [--db-path <path>] [--scan-id <id> | --last <N>]
```

#### `items`
```sh
fspulse report items [--db-path <path>] [--item-id <id> | --item-path <path> | --root-id <id>] [--invalid]
```

#### `changes`
```sh
fspulse report changes [--db-path <path>] [--change-id <id> | --item-id <id> | --scan-id <id>]
```

Notes:
- `--invalid` on `items` requires `--root-id`

### `query`
Executes a structured query using FsPulse's flexible syntax.

```sh
fspulse query [--db-path <path>] "<query string>"
```

Example:
```sh
fspulse query "items where item_path:('docs') order by mod_date desc limit 10"
```

See [Query Syntax](query.md) for full details.

---

## Shared Option: `--db-path`
Many commands support the optional `--db-path` parameter to specify a custom SQLite database location.

- If omitted, FsPulse defaults to the appropriate system-specific location using the [directories](https://docs.rs/directories) crate
- The database file is always named `fspulse.db`

Examples:
```sh
fspulse scan --root-path /home/user/data --db-path /tmp/fspulse
fspulse query --db-path /var/fspulse "changes where val_new:(I)"
```

---

See also: [Interactive Mode](interactive_mode.md) · [Query Syntax](query.md) · [Configuration](configuration.md)

