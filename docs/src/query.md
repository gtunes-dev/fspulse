# Query Syntax

fsPulse provides a flexible, SQL-like query language for exploring scan results. This language supports filtering, custom column selection, ordering, and limiting the number of results.

---

## Query Structure

Each query begins with one of the five supported domains:

- `roots`
- `scans`
- `items`
- `versions`
- `hashes`

You can then add any of the following optional clauses:

```text
DOMAIN [WHERE ...] [GROUP BY ...] [SHOW ...] [ORDER BY ...] [LIMIT ...] [OFFSET ...]
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
| `scan_state`    | Scan State Enum | Yes     | State of the scan                              |
| `is_hash`       | Boolean         | Yes     | Hash new or changed files                      |
| `hash_all`      | Boolean         | No      | Hash all items including unchanged             |
| `is_val`        | Boolean         | Yes     | Validate new or changed files                  |
| `file_count`    | Integer         | Yes     | Count of files found in the scan               |
| `folder_count`  | Integer         | Yes     | Count of directories found in the scan         |
| `total_size`    | Integer         | Yes     | Total size in bytes of all files               |
| `new_hash_suspect_count` | Integer | No   | New suspect hashes detected in this scan       |
| `new_val_invalid_count` | Integer  | No   | New validation failures detected in this scan  |
| `add_count`     | Integer         | Yes     | Number of items added in the scan              |
| `modify_count`  | Integer         | Yes     | Number of items modified in the scan           |
| `delete_count`  | Integer         | Yes     | Number of items deleted in the scan            |
| `val_unknown_count` | Integer     | No      | Files with unknown validation state            |
| `val_valid_count` | Integer       | No      | Files with valid validation state              |
| `val_invalid_count` | Integer     | No      | Files with invalid validation state            |
| `val_no_validator_count` | Integer | No     | Files with no available validator              |
| `hash_unknown_count` | Integer    | No      | Files with unknown hash state                  |
| `hash_baseline_count` | Integer   | No      | Files with baseline hash state                 |
| `hash_suspect_count` | Integer | No      | Files with suspect hash state               |
| `error`         | String          | No      | Error message if scan failed                   |

---

### `items` Domain

The `items` domain queries item identity — the permanent properties of each tracked file or directory.

| Column            | Type              | Default | Description                              |
|-------------------|-------------------|---------|------------------------------------------|
| `item_id`         | Integer           | Yes     | Unique item identifier                   |
| `root_id`         | Integer           | Yes     | Root directory identifier                |
| `item_path`       | Path              | Yes     | Full path of the item                    |
| `item_name`       | Path              | Yes     | Filename or directory name (last segment)|
| `file_extension`  | String            | Yes     | Lowercase file extension (null for folders/extensionless) |
| `item_type`       | Item Type Enum    | Yes     | File, Directory, Symlink, or Unknown     |
| `has_validator`   | Boolean           | No      | True if a structural validator exists for this file type |
| `do_not_validate` | Boolean           | No      | True if user has opted this item out of validation |

---

### `versions` Domain

The `versions` domain queries individual item version rows — each representing a distinct state of an item over a temporal range. Filter with `is_current:(T)` to query only the latest version of each item.

| Column            | Type              | Default | Description                              |
|-------------------|-------------------|---------|------------------------------------------|
| `item_version`    | Integer           | Yes     | Version number (per-item sequence)       |
| `item_id`         | Integer           | Yes     | Item this version belongs to             |
| `root_id`         | Integer           | Yes     | Root directory identifier                |
| `item_path`       | Path              | Yes     | Full path of the item                    |
| `item_name`       | Path              | No      | Filename or directory name (last segment)|
| `file_extension`  | String            | No      | Lowercase file extension (null for folders/extensionless) |
| `item_type`       | Item Type Enum    | Yes     | File, Directory, Symlink, or Unknown     |
| `first_scan_id`   | Integer           | Yes     | Scan where this version was first observed |
| `last_scan_id`    | Integer           | Yes     | Last scan confirming this version's state|
| `is_added`        | Boolean           | No      | True if item was added in this version   |
| `is_deleted`      | Boolean           | Yes     | True if item was deleted in this version |
| `is_current`      | Boolean           | No      | True if this is the latest version of the item |
| `access`          | Access Status     | No      | Access state                             |
| `mod_date`        | Date              | Yes     | Last modification date                   |
| `size`            | Integer           | Yes     | File size in bytes                       |
| `add_count`       | Integer           | No      | Descendant items added (folders only; null for files) |
| `modify_count`    | Integer           | No      | Descendant items modified (folders only; null for files) |
| `delete_count`    | Integer           | No      | Descendant items deleted (folders only; null for files) |
| `unchanged_count` | Integer           | No      | Descendant items unchanged (folders only; null for files) |
| `val_scan_id`     | Id                | No      | Scan in which this version was validated (NULL if not yet validated; may differ from `first_scan_id`) |
| `val_state`       | Validation Status | No      | Validation state (files only; null for folders) |
| `val_error`       | String            | No      | Validation error message (files only; null for folders) |
| `val_reviewed_at` | Date              | No      | Timestamp when user marked a validation issue as reviewed (NULL until reviewed) |
| `hash_reviewed_at`| Date              | No      | Timestamp when user marked a hash integrity issue as reviewed (NULL until reviewed) |

---

### `hashes` Domain

The `hashes` domain queries hash observation records — each representing a SHA-256 hash computed for an item version during a scan.

| Column            | Type              | Default | Description                              |
|-------------------|-------------------|---------|------------------------------------------|
| `item_id`         | Integer           | Yes     | Item this hash belongs to                |
| `item_version`    | Integer           | Yes     | Version this hash was observed on        |
| `item_path`       | Path              | Yes     | Full path of the item                    |
| `item_name`       | Path              | No      | Filename or directory name (last segment)|
| `first_scan_id`   | Integer           | Yes     | Scan where this hash was first observed  |
| `last_scan_id`    | Integer           | Yes     | Last scan confirming this hash           |
| `file_hash`       | Hash              | Yes     | SHA-256 content hash (hex)               |
| `hash_state`      | Hash State        | Yes     | Baseline or Suspect                      |

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
| Date                | `2024-01-01`, `2024-01-01 14:30:00`, `1711929600`, `null`, `not null` | Three input forms (see below). Ranges are inclusive.          |
| Boolean             | `true`, `false`, `T`, `F`, `null`, `not null`         | Unquoted.                                                             |
| String              | `'example'`, `'error: missing EOF'`, `null`, `not null` | Quoted strings.                                                     |
| Path                | `'photos/reports'`, `'file.txt'`                      | Must be quoted. **Null values are not supported.**                    |
| Validation Status   | `V`, `I`, `N`, `U`, `null`, `not null`                 | Valid, Invalid, No Validator, Unknown. Null for folders. Unquoted.     |
| Hash State          | `V`, `S`, `U`, `null`, `not null`                      | Valid, Suspect, Unknown. Null for folders. Unquoted.               |
| Item Type Enum      | `F`, `D`, `S`, `U`                                    | File, Directory, Symlink, Unknown. Unquoted.                          |
| Scan State Enum     | `S`, `W`, `AF`, `AS`, `C`, `P`, `E`                   | Scanning, Sweeping, Analyzing Files, Analyzing Scan, Completed, Stopped, Error. `A` is shorthand for `AF`. Unquoted. |
| Access Status       | `N`, `M`, `R`                                         | No Error, Meta Error, Read Error. Unquoted.                           |

### Date Filter Formats

Date columns accept three input forms, matching the three display formats available via `@short`, `@full`, and `@timestamp`. Any value produced by a query can be used directly as filter input.

| Form | Example | Behavior |
|------|---------|----------|
| Date only | `2025-01-15` | Matches the **entire day** (00:00:00 through 23:59:59 local time) |
| Date and time | `2025-01-15 14:30:00` | Matches that **exact second** |
| Unix epoch | `1737936000` | Matches that **exact second** (10+ digits, UTC) |

These forms can be mixed freely within a filter or range:

```text
# Date-only range
started_at:(2025-01-01..2025-01-31)

# Exact time range
started_at:(2025-01-15 08:00:00..2025-01-15 17:00:00)

# Mixed forms in a range
started_at:(2025-01-15..2025-01-16 14:30:00)
mod_date:(1737936000..2025-02-01)

# Multiple values (OR'd)
started_at:(2025-01-15, 2025-02-01 09:00:00, 1737936000)
```

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
| Validation / Hash State / Enum / Boolean  | `full`, `short`                          |
| Integer / String             | *(no formatting options)*                         |

All three date display formats (`@short`, `@full`, `@timestamp`) produce values that can be used directly as date filter input — see [Date Filter Formats](#date-filter-formats) above.

---

## The `GROUP BY` Clause

Groups rows by one or more columns and enables aggregate functions in the `SHOW` clause. When `GROUP BY` is used, a `SHOW` clause is required.

```text
versions where is_current:(T), root_id:(1) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc
```

### Aggregate Functions

| Function | Applies To | Description |
|----------|-----------|-------------|
| `count(*)` | Any | Count all rows in the group |
| `count(col)` | Any column | Count non-null values |
| `sum(col)` | Integer columns | Sum of values |
| `avg(col)` | Integer columns | Average of values |
| `min(col)` | Integer, Date, Id columns | Minimum value |
| `max(col)` | Integer, Date, Id columns | Maximum value |

### Rules

- Every non-aggregate column in `SHOW` must also appear in `GROUP BY`
- Aggregate functions can be used in `ORDER BY` (e.g., `order by count(*) desc`)

---

## The `ORDER BY` Clause

Specifies sort order for the results. Supports both column names and aggregate expressions.

```text
items order by mod_date desc, item_path asc
scans group by root_id show root_id, count(*) order by count(*) desc
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

# All PDF items
items where file_extension:('pdf')

# Current state of large files, sorted by size
versions where is_current:(T), item_type:(F), size:(> 1048576) show item_path, size order by size desc

# Version history for a specific item
versions where item_id:(42) order by first_scan_id

# Deleted versions across all roots
versions where is_deleted:(true) show item_path, item_type, first_scan_id, last_scan_id

# Versions with validation failures
versions where val_state:(I) show default, val_error order by first_scan_id desc

# Suspect hash observations
hashes where hash_state:(S) show item_path, item_version, file_hash

# All hash observations for a specific item
hashes where item_id:(42) order by first_scan_id

# Scans with timestamps for programmatic processing
scans show scan_id, started_at@timestamp, file_count order by started_at desc limit 10

# Scans with change and integrity counts
scans show scan_id, file_count, total_size, add_count, modify_count, delete_count, new_hash_suspect_count, new_val_invalid_count order by started_at desc

# File count and total size by extension
versions where is_current:(T), root_id:(1), item_type:(F) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc

# Scan count per root
scans group by root_id show root_id, count(*), max(total_size), max(file_count) order by count(*) desc

# Hash state distribution
hashes group by hash_state show hash_state, count(*)

# Validation failures by root
versions where val_state:(I) group by root_id show root_id, count(*)
```

---

See also: [Data Explorer](web_ui/data_explorer.md) · [Validators](validators.md) · [Configuration](configuration.md)
