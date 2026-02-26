# Query Syntax

FsPulse provides a flexible, SQL-like query language for exploring scan results. This language supports filtering, custom column selection, ordering, and limiting the number of results.

---

## Query Structure

Each query begins with one of the five supported domains:

- `roots`
- `scans`
- `items`
- `versions`
- `alerts`

You can then add any of the following optional clauses:

```text
DOMAIN [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...] [OFFSET ...]
```

---

## Column Availability

Each domain has a set of available columns. Columns marked as **default** are shown when no `SHOW` clause is specified.

### `roots` Domain

| Column      | Type    | Default |
|-------------|---------|---------|
| `root_id`   | Integer | Yes     |
| `root_path` | Path    | Yes     |

---

### `scans` Domain

| Column          | Type            | Default | Description                                    |
|-----------------|-----------------|---------|------------------------------------------------|
| `scan_id`       | Integer         | Yes     | Unique scan identifier                         |
| `root_id`       | Integer         | Yes     | Root directory identifier                      |
| `schedule_id`   | Integer         | Yes     | Schedule identifier (null for manual scans)    |
| `started_at`    | Date            | Yes     | Timestamp when scan started                    |
| `ended_at`      | Date            | Yes     | Timestamp when scan ended (null if incomplete) |
| `was_restarted` | Boolean         | Yes     | True if scan was resumed after restart         |
| `scan_state`    | Scan State Enum | No      | State of the scan                              |
| `is_hash`       | Boolean         | Yes     | Hash new or changed files                      |
| `hash_all`      | Boolean         | No      | Hash all items including unchanged             |
| `is_val`        | Boolean         | Yes     | Validate new or changed files                  |
| `val_all`       | Boolean         | No      | Validate all items including unchanged         |
| `file_count`    | Integer         | Yes     | Count of files found in the scan               |
| `folder_count`  | Integer         | Yes     | Count of directories found in the scan         |
| `total_size`    | Integer         | Yes     | Total size in bytes of all files               |
| `alert_count`   | Integer         | Yes     | Number of alerts created during the scan       |
| `add_count`     | Integer         | Yes     | Number of items added in the scan              |
| `modify_count`  | Integer         | Yes     | Number of items modified in the scan           |
| `delete_count`  | Integer         | Yes     | Number of items deleted in the scan            |
| `error`         | String          | No      | Error message if scan failed                   |

---

### `items` Domain

The `items` domain queries each item's **latest version** — the most recent known state. Identity columns come from the `items` table; state columns come from the item's current version.

| Column          | Type              | Default | Description                              |
|-----------------|-------------------|---------|------------------------------------------|
| `item_id`       | Integer           | Yes     | Unique item identifier                   |
| `root_id`       | Integer           | Yes     | Root directory identifier                |
| `item_path`     | Path              | Yes     | Full path of the item                    |
| `item_name`     | Path              | No      | Filename or directory name (last segment)|
| `item_type`     | Item Type Enum    | Yes     | File, Directory, Symlink, or Unknown     |
| `version_id`    | Integer           | No      | Current version identifier               |
| `first_scan_id` | Integer           | No      | Scan where current version first appeared|
| `last_scan_id`  | Integer           | Yes     | Last scan confirming current state       |
| `is_deleted`    | Boolean           | Yes     | True if item is currently deleted        |
| `access`        | Access Status     | No      | Access state (NoError, MetaError, ReadError) |
| `mod_date`      | Date              | Yes     | Last modification date                   |
| `size`          | Integer           | No      | File size in bytes                       |
| `last_hash_scan`| Integer           | No      | Last scan that evaluated the hash        |
| `file_hash`     | String            | No      | SHA-256 content hash                     |
| `last_val_scan` | Integer           | No      | Last scan that evaluated validation      |
| `val`           | Validation Status | No      | Validation state                         |
| `val_error`     | String            | No      | Validation error message                 |

---

### `versions` Domain

The `versions` domain queries individual item version rows — each representing a distinct state of an item over a temporal range. Use this domain to explore item history and state changes.

| Column          | Type              | Default | Description                              |
|-----------------|-------------------|---------|------------------------------------------|
| `version_id`    | Integer           | Yes     | Unique version identifier                |
| `item_id`       | Integer           | Yes     | Item this version belongs to             |
| `root_id`       | Integer           | Yes     | Root directory identifier                |
| `item_path`     | Path              | No      | Full path of the item                    |
| `item_name`     | Path              | No      | Filename or directory name (last segment)|
| `item_type`     | Item Type Enum    | Yes     | File, Directory, Symlink, or Unknown     |
| `first_scan_id` | Integer           | Yes     | Scan where this version was first observed |
| `last_scan_id`  | Integer           | Yes     | Last scan confirming this version's state|
| `is_deleted`    | Boolean           | Yes     | True if item was deleted in this version |
| `access`        | Access Status     | No      | Access state                             |
| `mod_date`      | Date              | No      | Last modification date                   |
| `size`          | Integer           | No      | File size in bytes                       |
| `last_hash_scan`| Integer           | No      | Last scan that evaluated the hash        |
| `file_hash`     | String            | No      | SHA-256 content hash                     |
| `last_val_scan` | Integer           | No      | Last scan that evaluated validation      |
| `val`           | Validation Status | No      | Validation state                         |
| `val_error`     | String            | No      | Validation error message                 |

---

### `alerts` Domain

| Column          | Type              | Default | Description                              |
|-----------------|-------------------|---------|------------------------------------------|
| `alert_id`      | Integer           | No      | Unique alert identifier                  |
| `alert_type`    | Alert Type Enum   | Yes     | Type of alert                            |
| `alert_status`  | Alert Status Enum | Yes     | Current status (Open, Flagged, Dismissed)|
| `root_id`       | Integer           | No      | Root directory identifier                |
| `scan_id`       | Integer           | No      | Scan that generated the alert            |
| `item_id`       | Integer           | No      | Item the alert is about                  |
| `item_path`     | Path              | Yes     | Path of the affected item                |
| `created_at`    | Date              | Yes     | When the alert was created               |
| `updated_at`    | Date              | No      | When the alert status was last changed   |
| `prev_hash_scan`| Integer           | No      | Previous hash scan (for suspicious hash) |
| `hash_old`      | String            | No      | Previous hash value                      |
| `hash_new`      | String            | No      | New hash value                           |
| `val_error`     | String            | Yes     | Validation error message                 |

---

## The `WHERE` Clause

The `WHERE` clause filters results using one or more filters. Each filter has the structure:

```text
column_name:(value1, value2, ...)
```

Values must match the column's type. You can use individual values, ranges (when supported), or a comma-separated combination.

| Type                | Examples                                              | Notes                                                                 |
|---------------------|-------------------------------------------------------|-----------------------------------------------------------------------|
| Integer             | `5`, `1..5`, `3, 5, 7..9`, `> 1024`, `< 10`, `null`, `not null` | Supports ranges, comparators, and nullability. Ranges are inclusive. |
| Date                | `2024-01-01`, `2024-01-01..2024-06-30`, `null`, `not null` | Use `YYYY-MM-DD`. Ranges are inclusive.                                |
| Boolean             | `true`, `false`, `T`, `F`, `null`, `not null`         | Unquoted.                                                             |
| String              | `'example'`, `'error: missing EOF'`, `null`, `not null` | Quoted strings.                                                     |
| Path                | `'photos/reports'`, `'file.txt'`                      | Must be quoted. **Null values are not supported.**                    |
| Validation Status   | `V`, `I`, `N`, `U`                                    | Valid, Invalid, No Validator, Unknown. Unquoted.                      |
| Item Type Enum      | `F`, `D`, `S`, `U`                                    | File, Directory, Symlink, Unknown. Unquoted.                          |
| Alert Type Enum     | `H`, `I`, `A`                                         | Suspicious Hash, Invalid Item, Access Denied. Unquoted.               |
| Alert Status Enum   | `O`, `F`, `D`                                         | Open, Flagged, Dismissed. Unquoted.                                   |
| Scan State Enum     | `S`, `W`, `AF`, `AS`, `C`, `P`, `E`                   | Scanning, Sweeping, Analyzing Files, Analyzing Scan, Completed, Stopped, Error. `A` is shorthand for `AF`. Unquoted. |
| Access Status       | `N`, `M`, `R`                                         | No Error, Meta Error, Read Error. Unquoted.                           |

---

## Combining Filters

When specifying multiple values within a single filter, the match is logically **OR**. When specifying multiple filters across different columns, the match is logically **AND**.

For example:

```text
scans where started_at:(2025-01-01..2025-01-07, 2025-02-01..2025-02-07), is_hash:(T)
```

This query matches scans that:
- Occurred in **either** the first week of January 2025 **or** the first week of February 2025
- **AND** were performed with hashing enabled

---

## The `SHOW` Clause

The `SHOW` clause controls which columns are displayed and how some of them are formatted. If omitted, a default column set is used.

You may specify:

- A list of column names
- The keyword `default` to insert the default set
- The keyword `all` to show all available columns

Formatting modifiers can be applied using the `@` symbol:

```text
item_path@name, mod_date@short
```

### Format Specifiers by Type

| Type                         | Allowed Format Modifiers                          |
|------------------------------|---------------------------------------------------|
| Date                         | `full`, `short`, `timestamp`                      |
| Path                         | `full`, `relative`, `short`, `name`               |
| Validation / Enum / Boolean  | `full`, `short`                                   |
| Integer / String             | *(no formatting options)*                         |

The `timestamp` format modifier converts dates to UTC timestamps (seconds since Unix epoch), which is useful for programmatic processing or web applications that need to format dates in the user's local timezone.

---

## The `ORDER BY` Clause

Specifies sort order for the results:

```text
items order by mod_date desc, item_path asc
```

If direction is omitted, `ASC` is assumed.

---

## The `LIMIT` and `OFFSET` Clauses

`LIMIT` restricts the number of rows returned. `OFFSET` skips a number of rows before returning results.

```text
items limit 50 offset 100
```

---

## Examples

```text
# Items whose path contains 'reports'
items where item_path:('reports')

# Large files sorted by size
items where item_type:(F), size:(> 1048576) show default, size order by size desc

# Version history for a specific item
versions where item_id:(42) order by first_scan_id

# Deleted items across all roots
items where is_deleted:(true)

# Versions with validation failures
versions where val:(I) show default, val_error order by first_scan_id desc

# Open or flagged alerts for suspicious hashes
alerts where alert_type:(H), alert_status:(O, F) order by created_at desc

# Scans with timestamps for programmatic processing
scans show scan_id, started_at@timestamp, file_count order by started_at desc limit 10

# Scans with change and alert counts
scans show scan_id, file_count, total_size, add_count, modify_count, delete_count, alert_count order by started_at desc
```

---

See also: [Explore Page](web_ui/explore.md) · [Validators](validators.md) · [Configuration](configuration.md)
