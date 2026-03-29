# Tools

The MCP server provides 10 tools. The AI agent selects which tools to call based on your prompt.

## system_overview

High-level summary of all monitored roots, their latest scan status, unreviewed integrity issue counts, and database size.

## query_data

Execute a query using the fsPulse [query DSL](../query.md). Supports all five domains (items, versions, hashes, scans, roots), filtering, aggregation with GROUP BY, and ordering. Returns results as a formatted table.

## query_count

Count rows matching a query without returning the data.

## query_help

Returns documentation for the query DSL, including available columns and filter syntax for each domain. The agent uses this to learn valid column names before constructing queries.

## integrity_report

Report of items with integrity issues (validation failures, suspect hashes) for a specific root. Supports filtering by issue type, review status, file extension, and path.

## scan_history

Scan history for a root showing file counts, sizes, change rates, and integrity findings over time.

## browse_filesystem

Browse the filesystem tree at a specific point in time. Lists immediate children of a directory within a root at a given scan.

## search_files

Search for files and directories by name within a root at a specific point in time.

## item_detail

Detailed information about a specific item including its version history, size changes, and integrity state.

## scan_changes

Show what files were added, modified, or deleted in a specific scan. Can filter by change type.
