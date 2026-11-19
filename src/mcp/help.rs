use crate::query::columns::*;

pub(super) fn general_help() -> String {
    r#"## fspulse Data Model

fsPulse tracks filesystem state over time using temporal versioning:

- **roots** — Monitored directory paths. Each root is tracked independently.
- **items** — Stable identity of a file or directory (path, name, type). Mutable state lives in versions, not here.
- **versions** — Each version captures the full state of an item (size, mod_date, val_state, etc.) from `first_seen_at` until the item's next version begins. Filter with `is_current:(T)` for latest state. **IMPORTANT**: `is_current:(T)` returns the latest version of *every* item, including items whose latest version is a deletion record (`is_deleted:(T)`). To analyze only live (non-deleted) items, always combine both filters: `is_current:(T), is_deleted:(F)`. `is_deleted:(T)` also covers items that are merely ignored; see "Ignore patterns" below.
- **hashes** — SHA-256 hash observations on file versions. Hash state is Baseline (expected) or Suspect (hash changed without metadata change).
- **snapshots** — One row per root per hour in which something changed: items added, modified and deleted in that hour, and the root's file count, folder count and total size at the end of it. Hours with no changes have no row. Rows of hours in which an integrity check completed also carry integrity counts. Rows from before the realtime model are one per old scan, at the scan's start time.
- **scans** — Integrity checks: runs that hash and/or validate a root's files. They do not detect changes.

Relationships:
- A root has zero or more items, snapshots and integrity checks.
- An item always has at least one version.
- A version has zero or more hash observations (0 = file never hashed, 1 = baseline hash, >1 = the hash changed while size and mod_date did not — later observations may be suspect).

How changes are recorded:
- fsPulse watches each root and records a change within seconds of it happening. A periodic full check of the root (at startup, then daily, or every few hours when watching is not reliable) catches anything the watcher missed.
- Changes are grouped by the hour. If an item changes several times within one hour, that is one version: `first_seen_at` is when the first of those changes was recorded, `last_changed_at` the latest, and `change_count` how many were folded in. A change in a later hour starts a new version. Once a version has been hashed or validated it is never updated; a further change starts a new version even within the same hour.
- Because of this, state can be reconstructed exactly at any hour boundary and at "now", but not at an arbitrary minute in the past. The time-based tools move range starts, and ends in the past, back to the hour and report the times they used.
- A range compares its two ends: an item is added, modified or deleted between them, or "transient" if it appeared and disappeared inside the range.

Ignore patterns:
- Each root can have ignore patterns (for example `node_modules`, `*.tmp`, `Library/Caches`). A path that matches is not tracked. An item first seen while a pattern matches it is never recorded at all.
- An item that was already tracked when a pattern came to match it is **ignored**, not deleted: it gets a version with `is_deleted:(T), is_ignored:(T)` ("tracking stopped") and its earlier history is kept. Nothing is known about it from then on, including whether it still exists.
- If the pattern is removed and the item is still on disk, it gets a version with `is_added:(T), is_ignored:(T)` ("tracking resumed"). If it is no longer on disk, it gets an ordinary deletion.
- Ignored and resumed items are never counted as deleted or added: not in the time-based tools, and not in `add_count` / `delete_count` on snapshots (see `ignored_count`, `resumed_count`).
- In version queries, `is_deleted:(T)` means "not present" and so includes ignored items. For real deletions use `is_deleted:(T), is_ignored:(F)`; for currently ignored items use `is_current:(T), is_deleted:(T), is_ignored:(T)`.
- Items already tracked are relabelled by the next full check after a pattern changes, so for a while after a change some matching items may still read as present.

Integrity checks:
- Run on a schedule or on demand, separately from change tracking. Two settings:
  - **Hashing** (`is_hash`, `hash_all` on scans): None, New/Changed (`is_hash:(T), hash_all:(F)` — hashes only versions that have never been hashed), or All (`is_hash:(T), hash_all:(T)` — recomputes hashes for every file). Only "All" checks can detect suspect hashes, because only they recompute and compare hashes for previously hashed versions.
  - **Validation** (`is_val` on scans): On or off. Checks the structural integrity of files that have a validator and have not yet been validated for this version. Validation is never repeated on an already-validated version.
- A file is hashed or validated only if its size and modification time on disk match the version, both before and after it is read. A file that is changing is skipped and picked up by the next check.
- If a version has no hash observations or `val_state` is Unknown, no integrity check with the appropriate setting has covered that version yet — it is not an error.

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
- **snapshots** — Hourly change counts and totals per root (taken_at, add/modify/delete counts, ignored/resumed counts, file/folder counts, total size, integrity counts)
- **scans** — Integrity checks (timestamps, settings, new integrity findings)
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

- **Date columns** (`mod_date`, `first_seen_at`, `last_changed_at`, `taken_at`, `started_at`, `ended_at`, `val_reviewed_at`, `hash_reviewed_at`, ...):
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

For other tools (`integrity_report`, `integrity_check_history`, `activity`, `changes`, `item_detail`, `browse_filesystem`):
- Each tool accepts `limit` (default 50, max 200) and `offset` (default 0) parameters
- Total counts are included in the response
- When more results are available, the response indicates the next offset to use

### Timestamps: Event Time vs. Recording Time

When answering "when did this change?", choose the right timestamp:

- **`mod_date`** (on versions) is the filesystem's own timestamp — when the file's content was last modified according to the file itself. It can be older than the change fsPulse saw (a file copied in keeps its original `mod_date`).
- **`first_seen_at`** and **`last_changed_at`** (on versions) are when fsPulse recorded the change. For a change the watcher saw, this is within seconds of the event. For a change found by a full check it is only an upper bound: the change happened some time before, since the previous full check. When `first_seen_at` is much later than `mod_date`, trust `mod_date` for when the content changed.
- **Deletes** have no `mod_date`. `first_seen_at` of the deletion version is when the deletion was recorded.
- **`started_at`** (on scans) is when an integrity check began. It says nothing about when files changed.

### Examples

```
versions where is_current:(T), is_deleted:(F), root_id:(1) show item_path, size limit 20
versions where is_current:(T), is_deleted:(F), root_id:(1, 2), item_type:(F) show item_path, size
hashes where hash_state:(S) show item_path, file_hash
scans where root_id:(1) order by started_at desc limit 10
snapshots where root_id:(1), taken_at:(2026-11-01..2026-11-17) show taken_at@full, add_count, modify_count, delete_count, total_size
versions where root_id:(1), first_seen_at:(2026-11-17) show item_path, first_seen_at@full, change_count, is_added, is_deleted
items where file_extension:('pdf') show item_path, item_name
versions where is_current:(T), is_deleted:(F), item_type:(F) group by file_extension show file_extension, count(*), sum(size) order by sum(size) desc
scans group by root_id show root_id, count(*) order by count(*) desc
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
        "snapshots" => &SNAPSHOTS_QUERY_COLS,
        "roots" => &ROOTS_QUERY_COLS,
        _ => {
            return Err(rmcp::ErrorData::invalid_params(
                format!("Unknown domain '{}'. Valid domains: items, versions, hashes, snapshots, scans, roots", domain),
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
