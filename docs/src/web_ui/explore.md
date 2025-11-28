# Explore

The Explore page provides both visual query building and free-form query capabilities for analyzing your FsPulse data.

## Overview

Explore offers two ways to query your data:

- **Structured tabs** (Roots, Scans, Items, Changes, Alerts) — Visual query builder with column selection, sorting, and filtering
- **Query tab** — Free-form query entry using FsPulse's query language

## Structured Query Tabs

The **Roots**, **Scans**, **Items**, **Changes**, and **Alerts** tabs provide a visual interface for building queries without writing query syntax.

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
| **Sort** | Click to cycle through ascending (↑), descending (↓), or no sort (⇅) |
| **Filter** | Add a filter condition for this column |

### Working with Columns

**Show/Hide Columns:**
Check or uncheck the box next to any column name to include or exclude it from results.

**Reorder Columns:**
Drag columns using the grip handle to change the display order in the results table.

**Sort Results:**
Click the sort control to cycle through:
- ⇅ No sort
- ↑ Ascending
- ↓ Descending

Only one column can be sorted at a time.

**Filter Data:**
Click the filter button (+) to add a filter condition. Active filters display as badges showing the filter value. Click the X to remove a filter.

**Reset:**
Click the reset button in the column header to restore all columns to their default visibility, order, and clear all filters and sorts.

## Query Tab

The **Query** tab provides a free-form interface for writing queries using FsPulse's SQL-inspired query language.

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
items where item_type:(F) show item_path, size limit 25
items where item_type:(F), size:(>1000000) show item_path, size order by size desc limit 20
alerts where alert_status:(O) show alert_type, item_path, created_at limit 15
```

## Query Domains

Both interfaces support querying five data domains:

| Domain | Description |
|--------|-------------|
| **roots** | Configured scan roots |
| **scans** | Scan metadata and statistics |
| **items** | Files and folders from the most recent scan |
| **changes** | Change records across all scans |
| **alerts** | Integrity issues and validation failures |

## When to Use Each Interface

**Use structured tabs when:**
- Exploring data without knowing the exact query syntax
- Quickly toggling columns to find relevant information
- Building simple filters and sorts visually

**Use the Query tab when:**
- Writing complex queries with multiple conditions
- Using advanced query features (comparisons, multiple filters with AND/OR)
- Reproducing a specific query you've used before
- Learning the query syntax with immediate feedback

## Query Syntax

For complete documentation on the query language including all operators, field names, and advanced features, see [Query Syntax](../query.md).
