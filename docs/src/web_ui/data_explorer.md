# Data Explorer

The Data Explorer provides both visual query building and free-form query capabilities for analyzing your fsPulse data. It is located in the utility section of the sidebar, designed for power users who need detailed data access beyond what the primary pages offer.

## Overview

Data Explorer offers two ways to query your data:

- **Structured tabs** (Roots, Scans, Items, Versions, Hashes) — Visual query builder with column selection, sorting, and filtering
- **Query tab** — Free-form query entry using fsPulse's query language

<!-- Screenshot: Data Explorer showing the structured Items tab with column selector and results -->
<!-- ![Data Explorer - Structured Query](screenshot-placeholder-data-explorer-structured.png) -->

## Structured Query Tabs

The **Roots**, **Scans**, **Items**, **Versions**, and **Hashes** tabs provide a visual interface for building queries without writing query syntax.

### Layout

Each structured tab displays:
- **Column selector panel** (left) — Configure which columns to display and how
- **Results table** (right) — View query results with pagination

### Column Controls

The column selector provides several controls for each available column:

| Control | Description |
|---------|-------------|
| **Checkbox** | Show or hide the column in results |
| **Drag handle** | Reorder columns by dragging |
| **Sort** | Click to cycle through ascending, descending, or no sort |
| **Filter** | Add a filter condition for this column |

### Working with Columns

**Show/Hide Columns:**
Check or uncheck the box next to any column name to include or exclude it from results.

**Reorder Columns:**
Drag columns using the grip handle to change the display order in the results table.

**Sort Results:**
Click the sort control to cycle through no sort, ascending, and descending. Only one column can be sorted at a time.

**Filter Data:**
Click the filter button to add a filter condition. Active filters display as badges showing the filter value. Click the X to remove a filter.

**Reset:**
Click the reset button in the column header to restore all columns to their default visibility, order, and clear all filters and sorts.

## Query Tab

The **Query** tab provides a free-form interface for writing queries using fsPulse's SQL-inspired query language.

<!-- Screenshot: Data Explorer Query tab with a query entered and results displayed -->
<!-- ![Data Explorer - Query Tab](screenshot-placeholder-data-explorer-query.png) -->

### Features

- **Query input** — Text area for entering queries
- **Execute** — Run the query (or press Cmd/Ctrl + Enter)
- **Example queries** — Expandable section with clickable sample queries
- **Documentation link** — Quick access to the full query syntax reference
- **Results table** — Paginated results display

### Example Queries

The Query tab includes sample queries you can click to populate the input:

```text
items limit 10
versions where is_current:(T) show item_path, size, mod_date limit 20
versions where is_current:(T), item_type:(F), size:(>1000000) show item_path, size order by size desc limit 20
versions where is_deleted:(T) show item_path, item_type, first_scan_id, last_scan_id order by last_scan_id desc limit 20
hashes where hash_state:(S) show item_path, item_version, file_hash limit 20
```

## Query Domains

Both interfaces support querying five data domains:

| Domain | Description |
|--------|-------------|
| **roots** | Configured scan roots |
| **scans** | Scan metadata and statistics |
| **items** | Item identity — permanent properties of tracked files and directories |
| **versions** | Item version history — one row per distinct state over time |
| **hashes** | Hash observations — SHA-256 integrity records for item versions |

## When to Use Each Interface

**Use structured tabs when:**
- Exploring data without knowing the exact query syntax
- Quickly toggling columns to find relevant information
- Building simple filters and sorts visually

**Use the Query tab when:**
- Writing complex queries with multiple conditions
- Using advanced query features (comparisons, ranges, multiple filters)
- Reproducing a specific query you've used before
- Learning the query syntax with immediate feedback

## Query Syntax

For complete documentation on the query language including all operators, column names, and advanced features, see [Query Syntax](../query.md).
