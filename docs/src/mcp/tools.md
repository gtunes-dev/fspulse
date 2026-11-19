# Tools

The MCP server provides 11 tools. The AI agent selects which tools to call based on your prompt.

## Times

fsPulse records changes as they happen and groups them by the hour, so the tools that look at the past take times rather than scan numbers. Wherever a tool takes a time, it accepts:

| Form | Example | Meaning |
|------|---------|---------|
| `now` | `now` | The present (the default for the end of a range) |
| A span | `24h`, `7d`, `2w` | That long ago |
| A date | `2026-11-17` | The start of that day, local time |
| A date and time | `2026-11-17 14:30` | That local time |
| Unix seconds | `1795000000` | That instant |

The past can be reconstructed exactly at hour boundaries. The start of a range, and an end that is in the past, are moved back to the hour; each response states the times it actually used. A range that ends "now" includes everything recorded so far in the current hour.

## Pagination

All tools return at most 200 rows per call. Most tools accept `limit` (default 50, max 200) and `offset` (default 0) parameters for pagination. Total counts are included in responses, with next-offset hints when more results are available.

For `query_data`, pagination is controlled via `LIMIT` and `OFFSET` in the query string itself. Use `query_count` to get total row counts before paginating.

## system_overview

Summary of all monitored roots. For each root: how it is being tracked (watching in real time, degraded, offline, and so on), when it was last checked in full, its current file count, folder count and total size, and when its last integrity check completed. Also unreviewed integrity issue counts and the database path and size.

## activity

How a root changed over time. Per day (the default) or per hour: how many items were added, modified and deleted, and the root's file count, folder count and total size at the end of the period. Only periods in which something changed are listed, newest first. Takes `from` (default: 7 days ago) and `to` (default: now).

Counts are items, not events: a file saved fifty times within one hour counts once for that hour.

## changes

What was added, modified or deleted in a root, or beneath one folder, between two times. Takes `from` (required), `to` (default: now), and optionally `folder_path` and `change_type`.

The result is the net difference between the two ends of the range. A file changed ten times is listed once, as modified, with the number of changes recorded. A file that was created and deleted inside the range is "transient"; these are counted in the totals and listed only when `change_type` is `transient`.

An item that stopped being tracked because it came to match one of the root's ignore patterns is "ignored", and one that is tracked again after that is "resumed". Neither is counted as deleted or added. Both are listed by default and can be selected with `change_type`.

## browse_filesystem

Lists the immediate children of a folder as of now, or as of a past time (`at`). With `from`, each child also shows how it changed between `from` and `at`, each folder shows the net changes beneath it, and children deleted in that range are listed.

Items that are not tracked because they match an ignore pattern are left out unless `include_ignored` is true. They are then listed as "ignored since" the time tracking stopped, as they were last recorded. An item that was never tracked before its pattern was added has no record and is never listed.

## search_files

Searches a root for files and folders by name, as of now or as of a past time (`at`). With `from`, results show how each item changed in the range and include items deleted in it. Returns up to `limit` matches. `include_ignored` works as for `browse_filesystem`.

## item_detail

Detailed information about a specific item, including its version history: when each version began, when it last changed, how many changes were folded into it, size, modification date and validation state. The version list supports pagination via `limit`/`offset`.

## integrity_report

Report of items with integrity issues (validation failures, suspect hashes) for a specific root. Supports filtering by issue type, review status, file extension, and path. Supports pagination via `limit`/`offset`. Returns total count.

## integrity_check_history

Completed integrity checks for a root, newest first: when each ran, whether it hashed and validated, and how many new suspect hashes and validation failures it found. Supports pagination via `limit`/`offset`.

## query_data

Execute a query using the fsPulse [query DSL](../query.md). Supports six domains (roots, items, versions, hashes, snapshots, scans), filtering, aggregation with GROUP BY, and ordering. `snapshots` holds the hourly change counts and totals that `activity` reads; `scans` holds integrity checks. Date columns can be displayed as date-only (`@short`), date+time (`@full`), or Unix epoch (`@timestamp`), and all three formats can be used as filter input. Results are capped at 200 rows; use `LIMIT` and `OFFSET` in the query string to paginate. Returns results as a formatted table.

## query_count

Count rows matching a query without returning the data. Useful for understanding data volumes before paginating with `query_data`.

## query_help

Returns documentation for the data model and the query DSL, including available columns and filter syntax for each domain. The agent uses this to learn valid column names before constructing queries.
