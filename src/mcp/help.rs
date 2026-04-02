use crate::query::columns::*;

pub(super) fn general_help() -> String {
    r#"## fspulse Data Model

fsPulse tracks filesystem state over time using temporal versioning:

- **roots** — Monitored directory paths. Each root is scanned independently.
- **scans** — A scan is a point-in-time snapshot of a root. Records file/folder counts, sizes, and change/integrity stats.
- **items** — Stable identity of a file or directory (path, name, type). Mutable state lives in versions, not here.
- **versions** — Each version captures the full state of an item at a point in time (size, mod_date, val_state, etc.). A new version is created only when state changes. Filter with `is_current:(T)` for latest state. **IMPORTANT**: `is_current:(T)` returns the latest version of *every* item, including items whose latest version is a deletion record (`is_deleted:(T)`). To analyze only live (non-deleted) items, always combine both filters: `is_current:(T), is_deleted:(F)`.
- **hashes** — SHA-256 hash observations on file versions. Hash state is Baseline (expected) or Suspect (hash changed without metadata change).

Relationships:
- A root has zero or more scans and zero or more items.
- An item always has at least one version (created on first scan).
- A version has zero or more hash observations (0 = file never hashed, 1 = baseline hash, >1 = hash changed between scans — later observations may be suspect).

Scan modes and integrity:
- Every scan walks the filesystem and detects adds, modifications, and deletes. This always happens regardless of settings.
- Integrity actions are optional per scan, controlled by two settings:
  - **Hashing** (`is_hash`, `hash_all` on scans): Three modes — None (no hashing), New/Changed (`is_hash:(T), hash_all:(F)` — hashes only versions that have never been hashed), or All (`is_hash:(T), hash_all:(T)` — recomputes hashes for every file). Only "All" scans can detect suspect hashes, because only they recompute and compare hashes for previously hashed versions.
  - **Validation** (`is_val` on scans): On or off. Validating scans check the structural integrity of files that have a validator and have not yet been validated for this version. Validation is never repeated on an already-validated version.
- If a version has no hash observations or `val_state` is Unknown, it means no scan with the appropriate integrity setting has run since that version was created — it is not an error.

Integrity review:
- Users can mark integrity issues (validation failures, suspect hashes) as reviewed. `val_reviewed_at` and `hash_reviewed_at` on versions record when this happened (NULL = not yet reviewed). Filter for unreviewed issues with `val_reviewed_at:(null)` or `hash_reviewed_at:(null)`.

## fspulse Query DSL

### Structure

```
DOMAIN [WHERE ...] [GROUP BY ...] [SHOW ...] [ORDER BY ...] [LIMIT ...] [OFFSET ...]
```

### Domains

- **items** — Item identity (path, name, extension, type)
- **versions** — Item versions over time (size, mod_date, val_state, etc.). Filter with `is_current:(T), is_deleted:(F)` for latest state of live items. Using `is_current:(T)` alone includes deleted items.
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
| Path | `'/photos'`, `'report.pdf'` — **see path matching note below** |
| Val State | `V`, `I`, `N`, `U` (Valid, Invalid, No Validator, Unknown) |
| Hash State | `V`, `S`, `U` (Valid, Suspect, Unknown) |
| Item Type | `F`, `D`, `S`, `U` (File, Directory, Symlink, Unknown) |

**Filter logic:** Values within a single filter's parentheses are OR'd; separate filters are AND'd and **must be comma-separated**.

```
versions where root_id:(1, 2), item_type:(F)
```
This means "root 1 OR root 2, AND files only." Multiple filters without commas will cause a parse error.

**Path matching note**: Path filters use substring matching. When filtering for items under a specific folder, always include the trailing path separator to avoid matching sibling folders with similar name prefixes. For example, `item_path:('/data/photos/')` matches only items under the `photos` folder, while `item_path:('/data/photos')` would also match items under `photos-old`, `photos-backup`, etc.

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

- **Date columns** (`mod_date`, `started_at`, `ended_at`, `created_at`, `updated_at`, `val_reviewed_at`, `hash_reviewed_at`):
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

### ORDER BY Clause

Sort results by one or more columns or aggregate expressions, separated by commas. Direction is `asc` (default) or `desc`.

```
items where root_id:(1) order by mod_date desc, item_path asc
scans group by root_id show root_id, count(*) order by count(*) desc
```

- Columns do not need to appear in the SHOW clause to be used in ORDER BY
- Duplicate columns in ORDER BY are not allowed
- Do not use format modifiers (`@short`, `@full`, `@timestamp`, `@name`) in ORDER BY — use bare column names only. Modifiers are a SHOW feature. Sorting always uses the full underlying value regardless of display format.

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

### Timestamps: Event Time vs. Detection Time

When answering "when did this change?", choose the right timestamp:

- **`mod_date`** (on versions) is the filesystem's own timestamp — the actual time the file was created or last modified. Prefer this for adds and modifications when building timelines.
- **`started_at`** (on scans) is when fspulse detected the change, which may lag the actual event by up to a full scan interval. Use scan time only as a fallback.
- **Deletes** are the exception: the file is gone, so there is no `mod_date`. The detecting scan's `started_at` is the only available time anchor — treat it as an upper bound, not the exact deletion time.

### Examples

```
versions where is_current:(T), is_deleted:(F), root_id:(1) show item_path, size limit 20
versions where is_current:(T), is_deleted:(F), root_id:(1, 2), item_type:(F) show item_path, size
hashes where hash_state:(S) show item_path, file_hash
scans where root_id:(1) order by started_at desc limit 10
items where file_extension:('pdf') show item_path, item_name
versions where is_current:(T), is_deleted:(F), item_type:(F) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc
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
    out.push_str("| Column | Type | Description | Filter Syntax |\n");
    out.push_str("|--------|------|-------------|---------------|\n");

    for (name, spec) in col_map.entries() {
        let type_info = spec.col_type.info();
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            name,
            type_info.type_name,
            spec.description,
            type_info.tip.replace('\n', " "),
        ));
    }

    Ok(out)
}
