use crate::query::columns::*;

pub(super) fn general_help() -> String {
    r#"## fspulse Data Model

fsPulse tracks filesystem state over time using temporal versioning:

- **roots** — Monitored directory paths. Each root is scanned independently.
- **scans** — A scan is a point-in-time snapshot of a root. Records file/folder counts, sizes, and change/integrity stats.
- **items** — Stable identity of a file or directory (path, name, type). Mutable state lives in versions, not here.
- **versions** — Each version captures the full state of an item at a point in time (size, mod_date, val_state, etc.). A new version is created only when state changes. Filter with `is_current:(T)` for latest state.
- **hashes** — SHA-256 hash observations on file versions. Hash state is Baseline (expected) or Suspect (hash changed without metadata change).

Relationships:
- A root has zero or more scans and zero or more items.
- An item always has at least one version (created on first scan).
- A version has zero or more hash observations (0 = file never hashed, 1 = baseline hash, >1 = hash changed between scans — later observations may be suspect).

## fspulse Query DSL

### Structure

```
DOMAIN [WHERE ...] [GROUP BY ...] [SHOW ...] [ORDER BY ...] [LIMIT ...] [OFFSET ...]
```

### Domains

- **items** — Item identity (path, name, extension, type)
- **versions** — Item versions over time (size, mod_date, val_state, etc.). Filter with `is_current:(T)` for latest state.
- **hashes** — Hash observations on item versions (file_hash, hash_state)
- **scans** — Scan sessions (timestamps, counts, integrity findings)
- **roots** — Monitored root directories

### WHERE Clause

Filters use the syntax: `column_name:(value1, value2, ...)`

| Type | Examples |
|------|----------|
| Integer | `5`, `1..5`, `> 1024`, `null`, `not null` |
| Date | `2024-01-01`, `2024-01-01 14:30:00`, `1711929600` |
| Boolean | `T`, `F`, `true`, `false` |
| String | `'example'`, `null`, `not null` |
| Path | `'/photos'`, `'report.pdf'` |
| Val State | `V`, `I`, `N`, `U` (Valid, Invalid, No Validator, Unknown) |
| Hash State | `V`, `S`, `U` (Valid, Suspect, Unknown) |
| Item Type | `F`, `D`, `S`, `U` (File, Directory, Symlink, Unknown) |

Multiple values within a filter are OR'd. Multiple filters are AND'd.

### GROUP BY and Aggregates

Group rows by one or more columns and apply aggregate functions. GROUP BY requires a SHOW clause.

Aggregate functions: `count(*)`, `count(col)`, `sum(col)`, `avg(col)`, `min(col)`, `max(col)`
- `sum` and `avg` work on integer columns only
- `min` and `max` work on integer, date, and id columns
- Every non-aggregate column in SHOW must appear in GROUP BY
- Aggregates can be used in ORDER BY

### SHOW Clause

Controls displayed columns. Use `default` for defaults, `all` for everything.

**Format modifiers** — append `@mode` to a column name to control display format:

- **Date columns** (`mod_date`, `started_at`, `ended_at`, `created_at`, `updated_at`):
  - `@short` (default) — date only: `2026-03-30`
  - `@full` — date and time with second precision: `2026-03-30 18:44:11`
  - `@timestamp` — raw Unix epoch (seconds, UTC): `1743364800`
- **Path columns** (`item_path`):
  - `@name` — file/folder name only (no directory path)

Examples: `mod_date@full`, `started_at@timestamp`, `item_path@name`

**Date filter formats** — all three display formats above can be used as filter input, so output from a query can be fed directly back into a filter:

- Date only: `started_at:(2026-03-30)` — matches the entire day
- Date and time: `started_at:(2026-03-30 18:44:11)` — matches that exact second
- Unix epoch: `started_at:(1743364800)` — matches that exact second

These forms can be mixed freely in ranges: `started_at:(2026-03-30..2026-03-31 12:00:00)`, `mod_date:(1743364800..2026-04-01)`

### Pagination

All tools return at most 200 rows per call. Most tools accept `limit` and `offset` parameters to control pagination.

For `query_data`, pagination is controlled via LIMIT and OFFSET in the query string itself:
- Use `query_count` first to get the total row count for a query
- Then paginate with `limit N offset M` in the query string
- Results are capped at 200 rows regardless of the LIMIT value in the query

For other tools (`integrity_report`, `scan_history`, `scan_changes`, `item_detail`, `browse_filesystem`, `search_files`):
- Each tool accepts `limit` (default 50, max 200) and `offset` (default 0) parameters
- Total counts are included in the response
- When more results are available, the response indicates the next offset to use

### Examples

```
versions where is_current:(T), root_id:(1) show item_path, size limit 20
hashes where hash_state:(S) show item_path, file_hash
scans where root_id:(1) order by started_at desc limit 10
items where file_extension:('pdf') show item_path, item_name
versions where is_current:(T), item_type:(F) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc
scans group by root_id show root_id, count(*), max(total_size) order by count(*) desc
hashes group by hash_state show hash_state, count(*)
```

Pagination example:
```
items where root_id:(1) limit 50 offset 0
items where root_id:(1) limit 50 offset 50
```

Use `query_help` with a domain parameter for column details."#
        .to_string()
}

pub(super) fn domain_help(domain: &str) -> Result<String, rmcp::ErrorData> {
    let col_map: &ColMap = match domain {
        "items" => &ITEMS_QUERY_COLS,
        "versions" => &VERSIONS_QUERY_COLS,
        "hashes" => &HASHES_QUERY_COLS,
        "scans" => &SCANS_QUERY_COLS,
        "roots" => &ROOTS_QUERY_COLS,
        _ => {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Unknown domain '{}'. Valid domains: items, versions, hashes, scans, roots", domain),
                None,
            ));
        }
    };

    let mut out = format!("## `{}` Domain Columns\n\n", domain);
    out.push_str("| Column | Type | Filter Syntax |\n");
    out.push_str("|--------|------|---------------|\n");

    for (name, spec) in col_map.entries() {
        let type_info = spec.col_type.info();
        out.push_str(&format!(
            "| `{}` | {} | {} |\n",
            name,
            type_info.type_name,
            type_info.tip.replace('\n', " "),
        ));
    }

    Ok(out)
}
