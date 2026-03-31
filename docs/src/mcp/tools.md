# Tools

The MCP server provides 10 tools. The AI agent selects which tools to call based on your prompt.

## Pagination

All tools return at most 200 rows per call. Most tools accept `limit` (default 50, max 200) and `offset` (default 0) parameters for pagination. Total counts are included in responses, with next-offset hints when more results are available.

For `query_data`, pagination is controlled via `LIMIT` and `OFFSET` in the query string itself. Use `query_count` to get total row counts before paginating.

## system_overview

High-level summary of all monitored roots with latest scan stats (file/folder counts, total monitored size), unreviewed integrity issue counts, and database path/size.

## query_data

Execute a query using the fsPulse [query DSL](../query.md). Supports all five domains (items, versions, hashes, scans, roots), filtering, aggregation with GROUP BY, and ordering. Date columns can be displayed as date-only (`@short`), date+time (`@full`), or Unix epoch (`@timestamp`), and all three formats can be used as filter input. Results are capped at 200 rows; use `LIMIT` and `OFFSET` in the query string to paginate. Returns results as a formatted table.

## query_count

Count rows matching a query without returning the data. Useful for understanding data volumes before paginating with `query_data`.

## query_help

Returns documentation for the query DSL, including available columns and filter syntax for each domain. The agent uses this to learn valid column names before constructing queries.

## integrity_report

Report of items with integrity issues (validation failures, suspect hashes) for a specific root. Supports filtering by issue type, review status, file extension, and path. Supports pagination via `limit`/`offset`. Returns total count.

## scan_history

Scan history for a root showing file counts, sizes, change rates, and integrity findings over time. Supports pagination via `limit`/`offset`. Returns total count.

## browse_filesystem

Browse the filesystem tree at a specific point in time. Lists immediate children of a directory within a root at a given scan. Supports pagination via `limit`/`offset`. Returns total count.

## search_files

Search for files and directories by name within a root at a specific point in time. Supports pagination via `limit`/`offset`. Returns total count.

## item_detail

Detailed information about a specific item including its version history, size changes, and integrity state. Version list supports pagination via `limit`/`offset`.

## scan_changes

Show what files were added, modified, or deleted in a specific scan. Can filter by change type. Supports pagination via `limit`/`offset`. Returns total count.
