# Database

FsPulse uses an embedded [SQLite](https://sqlite.org) database to store all scan-related data. The database schema mirrors the core domain concepts used in FsPulse: roots, scans, items, and changes.

---

## Database Name and Location

The database file is always named:

```text
fspulse.db
```

### Data Directory

FsPulse uses a **data directory** to store application data including configuration, logs, and (by default) the database. The data directory location is determined by:

1. **`FSPULSE_DATA_DIR` environment variable** (if set) - overrides the default location
2. **Platform-specific default** - uses the [`directories`](https://docs.rs/directories) crate's project local directory:

| Platform | Value | Example |
|----------|-------|---------|
| Linux    | `$XDG_DATA_HOME/fspulse` or `$HOME/.local/share/fspulse` | `/home/alice/.local/share/fspulse` |
| macOS    | `$HOME/Library/Application Support/fspulse` | `/Users/Alice/Library/Application Support/fspulse` |
| Windows  | `{FOLDERID_LocalAppData}\fspulse\data` | `C:\Users\Alice\AppData\Local\fspulse\data` |
| Docker   | `/data` | `/data` |

**What's stored in the data directory:**
- Configuration file (`config.toml`)
- Log files (`logs/`)
- Database file (`fspulse.db`) - by default

**Note for Docker users:** The data directory defaults to `/data` and can be overridden with `FSPULSE_DATA_DIR`, but this is generally **not recommended** since you can map any host directory or Docker volume to `/data` instead.

### Default Database Location

By default, the database is stored in the data directory:

```text
<data_dir>/fspulse.db
```

For example:
```text
/home/alice/.local/share/fspulse/fspulse.db
```

---

## Custom Database Location

If you need to store the database **outside** the data directory (for example, on a different volume or network share), you can override the database directory specifically:

**Environment Variable:**
```sh
export FSPULSE_DATABASE_DIR=/path/to/custom/directory
fspulse serve
```

**Config File (`config.toml`):**
```toml
[database]
dir = "/path/to/custom/directory"
```

In both cases, FsPulse will store the database as `fspulse.db` inside the specified directory. **The filename cannot be changed** — only the directory is configurable.

**Database Location Precedence:**

1. `FSPULSE_DATABASE_DIR` environment variable (highest priority)
2. `[database].dir` in config.toml
3. Data directory (from `FSPULSE_DATA_DIR` or platform default)

**Important:** Configuration and logs always remain in the data directory, even when the database is moved to a custom location.

See the [Configuration - Database Settings](configuration.md#database-settings) section for more details.

---

## Schema Overview

The database schema is implemented using Rust and reflects the same logical structure used by the query interface:

- `roots` — scanned root directories
- `scans` — individual scan snapshots
- `items` — discovered files and folders with metadata
- `changes` — additions, deletions, and modifications between scans

The schema is versioned to allow future upgrades without requiring a full reset.

---

## Exploring the Database

Because FsPulse uses SQLite, you can inspect the database using any compatible tool, such as:

- [DB Browser for SQLite](https://sqlitebrowser.org)
- The `sqlite3` command-line tool
- SQLite integrations in many IDEs and database browsers

> ⚠️ **Caution:** Making manual changes to the database may affect FsPulse's behavior or stability. Read-only access is recommended.

---

FsPulse manages all internal data access automatically. Most users will not need to interact with the database directly.

