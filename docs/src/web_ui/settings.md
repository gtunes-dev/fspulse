# Settings

The Settings page provides application configuration, database management, and version information.

---

## Configuration

A table displays all configurable settings with their values from each source:

| Column | Description |
|--------|-------------|
| Setting | The setting name |
| Default | Built-in default value |
| Config File | Value from `config.toml` (if set) |
| Environment | Value from environment variable (if set) |

The **active value** (the one FsPulse actually uses) is highlighted with a green border. Configuration precedence follows: Environment variable > Config file > Default.

Settings that require a server restart to take effect are marked with a restart indicator.

### Editing Settings

Click **Edit** on any setting to modify its value in `config.toml`. The edit dialog provides:
- Input validation (numeric ranges, valid log levels, etc.)
- A **Delete** option to remove the setting from `config.toml` and revert to the default

See [Configuration](../configuration.md) for details on all available settings and environment variables.

---

## Database

Shows database statistics and maintenance tools:

- **Database path**: Location of the `fspulse.db` file
- **Total size**: Current size of the database file
- **Wasted space**: Space that can be reclaimed through compaction

### Compact Database

Over time, deletions and updates leave unused space in the SQLite database. The **Compact Database** button runs SQLite's VACUUM command to reclaim this space. This operation runs as a background task.

---

## About

Displays application metadata:

- FsPulse version
- Database schema version
- Build date and git commit information
- Links to documentation and the GitHub repository
