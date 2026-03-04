# Setup

The Setup page is the single destination for all fsPulse configuration: managing scan roots, creating schedules, editing application settings, and viewing system information. It is organized into four tabs.

---

## Roots

<!-- Screenshot: Setup page Roots tab showing the roots table with Add Root and Scan Now buttons -->
<!-- ![Setup - Roots](screenshot-placeholder-setup-roots.png) -->

### Adding a Root

1. Click **Add Root**
2. Enter the full filesystem path to monitor
3. Optionally provide a friendly name
4. Save

### Managing Roots

- **Scan Now**: Create a manual scan task for the root
- **Delete**: Remove the root (also removes associated schedules)
- View root statistics and last scan time

---

## Schedules

fsPulse supports flexible scheduling options for automated monitoring.

<!-- Screenshot: Setup page Schedules tab showing schedule list with enable/disable toggles -->
<!-- ![Setup - Schedules](screenshot-placeholder-setup-schedules.png) -->

### Schedule Types

- **Daily**: Run at a specific time each day
- **Weekly**: Run on specific days of the week at a chosen time
- **Monthly**: Run on a specific day of the month
- **Interval**: Run every N hours/minutes

### Creating a Schedule

1. Click **Add Schedule**
2. Select the root to scan
3. Choose schedule type and timing
4. Configure scan options:
   - Enable hashing (default: all files)
   - Enable validation (default: new/changed files)
5. Save

### Schedule Management

- **Enable/Disable**: Temporarily pause schedules without deleting them
- **Edit**: Modify timing or scan options
- **Delete**: Remove the schedule

Scans and other tasks are queued and executed sequentially to prevent resource conflicts. You can view upcoming and running tasks on the [Dashboard](dashboard.md).

---

## Configuration

A table displays all configurable settings with their values from each source:

| Column | Description |
|--------|-------------|
| Setting | The setting name |
| Default | Built-in default value |
| Config File | Value from `config.toml` (if set) |
| Environment | Value from environment variable (if set) |

The **active value** (the one fsPulse actually uses) is highlighted with a green border. Configuration precedence follows: Environment variable > Config file > Default.

Settings that require a server restart to take effect are marked with a restart indicator.

### Editing Settings

Click **Edit** on any setting to modify its value in `config.toml`. The edit dialog provides:
- Input validation (numeric ranges, valid log levels, etc.)
- A **Delete** option to remove the setting from `config.toml` and revert to the default

See [Configuration](../configuration.md) for details on all available settings and environment variables.

### Database

Shows database statistics and maintenance tools:

- **Database path**: Location of the `fspulse.db` file
- **Total size**: Current size of the database file
- **Wasted space**: Space that can be reclaimed through compaction

**Compact Database**: Over time, deletions and updates leave unused space in the SQLite database. The **Compact Database** button runs SQLite's VACUUM command to reclaim this space. This operation runs as a background task.

---

## About

Displays application metadata:

- fsPulse version
- Database schema version
- Build date and git commit information
- Links to documentation and the GitHub repository
