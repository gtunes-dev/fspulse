# Database

FsPulse uses an embedded [SQLite](https://sqlite.org) database to store all scan-related data. The database schema mirrors the core domain concepts used in FsPulse: roots, scans, items, and changes.

---

## Database Name and Location

The database file is always named:

```
fspulse.db
```

By default, FsPulse stores the database in the root of the user's home directory, as determined by the [`directories`](https://docs.rs/directories) crate.

| Platform | Base Location         | Example             |
|----------|------------------------|---------------------|
| Linux    | `$HOME`               | `/home/alice`       |
| macOS    | `$HOME`               | `/Users/Alice`      |
| Windows  | `{FOLDERID_Profile}`  | `C:\Users\Alice`   |

The full path might look like:

```
/home/alice/fspulse.db
```

---

## Custom Database Path

You can override the default location using the `--db-path` option:

```sh
fspulse --db-path /some/other/folder
```

In this case, FsPulse will look for (or create) a file named `fspulse.db` inside the specified folder. The filename cannot be changed — only the directory is configurable.

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

