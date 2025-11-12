# Query Syntax

FsPulse provides a flexible, SQL-like query language for exploring scan results. This language supports filtering, custom column selection, ordering, and limiting the number of results.

---

## Query Structure

Each query begins with one of the four supported domains:

- `roots`
- `scans`
- `items`
- `changes`

You can then add any of the following optional clauses:

```text
DOMAIN [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

---

## Column Availability

### `roots` Domain

All queries that retrieve root information begin with the keyword `roots`:

```text
roots [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

| Property    | Type    |
|-------------|---------|
| `root_id`   | Integer |
| `root_path` | Path    |

---

### `scans` Domain

All queries that retrieve scan information begin with the keyword `scans`:

```text
scans [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

| Property          | Type     | Description                                    |
|-------------------|----------|------------------------------------------------|
| `scan_id`         | Integer  | Unique scan identifier                         |
| `root_id`         | Integer  | Root directory identifier                      |
| `scan_state`      | Scan State Enum | State of the scan                       |
| `is_hash`         | Boolean  | Hash new or changed files                      |
| `hash_all`        | Boolean  | Hash all items including unchanged             |
| `is_val`          | Boolean  | Validate new or changed files                  |
| `val_all`         | Boolean  | Validate all items including unchanged         |
| `started_at`      | Date     | Timestamp when scan started                    |
| `ended_at`        | Date     | Timestamp when scan ended (null if incomplete) |
| `was_restarted`   | Boolean  | True if scan was resumed after restart         |
| `file_count`      | Integer  | Count of files found in the scan               |
| `folder_count`    | Integer  | Count of directories found in the scan         |
| `total_size`      | Integer  | Total size in bytes of all files in the scan   |
| `alert_count`     | Integer  | Number of alerts created during the scan       |
| `add_count`       | Integer  | Number of items added in the scan              |
| `modify_count`    | Integer  | Number of items modified in the scan           |
| `delete_count`    | Integer  | Number of items deleted in the scan            |
| `error`           | String   | Error message if scan failed                   |

---

### `items` Domain

All queries that retrieve item information begin with the keyword `items`:

```text
items [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

| Property         | Type                |
|------------------|---------------------|
| `item_id`        | Integer             |
| `scan_id`        | Integer             |
| `root_id`        | Integer             |
| `item_path`      | Path                |
| `item_type`      | Item Type Enum      |
| `last_scan`      | Integer             |
| `is_ts`          | Boolean             |
| `mod_date`       | Date                |
| `size`           | Integer             |
| `last_hash_scan` | Integer             |
| `file_hash`      | String              |
| `last_val_scan`  | Integer             |
| `val`            | Validation Status   |
| `val_error`      | String              |

---

### `changes` Domain

All queries that retrieve change history begin with the keyword `changes`:

```text
changes [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

| Property             | Type                |
|----------------------|---------------------|
| `change_id`          | Integer             |
| `root_id`            | Integer             |
| `scan_id`            | Integer             |
| `item_id`            | Integer             |
| `item_path`          | Path                |
| `change_type`        | Change Type Enum    |
| `is_undelete`        | Boolean             |
| `meta_change`        | Boolean             |
| `mod_date_old`       | Date                |
| `mod_date_new`       | Date                |
| `hash_change`        | Boolean             |
| `last_hash_scan_old` | Integer             |
| `hash_old`           | String              |
| `hash_new`           | String              |
| `val_change`         | Boolean             |
| `last_val_scan_old`  | Integer             |
| `val_old`            | Validation Status   |
| `val_new`            | Validation Status   |
| `val_error_old`      | String              |
| `val_error_new`      | String              |

---

## The `WHERE` Clause

The `WHERE` clause filters results using one or more filters. Each filter has the structure:

```text
column_name:(value1, value2, ...)
```

Values must match the column’s type. You can use individual values, ranges (when supported), or a comma-separated combination. Values are **not quoted** unless explicitly shown.

| Type                | Examples                                              | Notes                                                                 |
|---------------------|-------------------------------------------------------|-----------------------------------------------------------------------|
| Integer             | `5`, `1..5`, `3, 5, 7..9`, `null`, `not null`, `NULL`, `NOT NULL` | Supports ranges and nullability. Ranges are inclusive.                |
| Date                | `2024-01-01`, `2024-01-01..2024-06-30`, `null`, `not null`, `NULL`, `NOT NULL` | Use `YYYY-MM-DD`. Ranges are inclusive.                                |
| Boolean             | `true`, `false`, `T`, `F`, `null`, `not null`, `NULL`, `NOT NULL` | Values are unquoted. Null values are allowed in all-lower or all-upper case. |
| String              | `'example'`, `'error: missing EOF'`, `null`, `NULL`   | Quoted strings. Null values are allowed in all-lower or all-upper case.     |
| Path                | `'photos/reports'`, `'file.txt'`                      | Must be quoted. **Null values are not supported.**                    |
| Validation Status   | `V`, `I`, `N`, `U`, `null`, `not null`, `NULL`, `NOT NULL` | Valid (V), Invalid (I), No Validator (N), Unknown (U). Unquoted. Ranges not supported. |
| Item Type Enum      | `F`, `D`, `S`, `O`, `null`, `not null`, `NULL`, `NOT NULL` | File (F), Directory (D), Symlink (S), Other (O). Unquoted. Ranges not supported. |
| Change Type Enum    | `N`, `A`, `M`, `D`, `null`, `not null`, `NULL`, `NOT NULL` | No Change (N), Add (A), Modify (M), Delete (D). Unquoted. Ranges not supported. |
| Scan State Enum     | `S`, `W`, `A`, `C`, `P`, `E`, `null`, `not null`, `NULL`, `NOT NULL` | Scanning (S), Sweeping (W), Analyzing (A), Completed (C), Stopped (P), Error (E). Unquoted. Ranges not supported. |

---

## Combining Filters

When specifying multiple values within a single filter, the match is logically **OR**. When specifying multiple filters across different columns, the match is logically **AND**.

For example:

```text
scans where started_at:(2025-01-01..2025-01-07, 2025-02-01..2025-02-07), hashing:(T)
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

## The `LIMIT` Clause

Restricts the number of rows returned:

```text
items limit 50
```

---

## Examples

```text
# Items whose path contains 'reports'
items where item_path:('reports')

# Changes involving validation failures
changes where val_new:(I) show default, val_old, val_new order by change_id desc

# Scans with timestamp for programmatic processing
scans show scan_id, started_at@timestamp, file_count order by started_at desc limit 10

# Scans with changes and alerts
scans show scan_id, file_count, total_size, add_count, modify_count, delete_count, alert_count order by started_at desc
```

---

See also: [Interactive Mode](interactive_mode.md) · [Validators](validators.md) · [Configuration](configuration.md)

