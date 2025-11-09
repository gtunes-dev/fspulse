# Explore (Query Interface)

The Explore page provides a full-featured query interface for advanced data analysis using FsPulse's query language.

## Overview

Explore offers a terminal-like experience in the browser, allowing you to:
- Write queries using FsPulse's SQL-inspired syntax
- View results in formatted tables
- Save and load queries
- Export results

## Query Domains

You can query four different data types:

- **`items`**: Files and folders from the most recent scan
- **`changes`**: Change records across all scans
- **`scans`**: Scan metadata and statistics
- **`roots`**: Configured scan roots
- **`alerts`**: Integrity issues and validation failures

## Basic Query Examples

### Find Invalid Items
```
items where val:(I)
```

### Recent Large Files
```
items where size > 1GB and item_type:(F)
```

### Hash Changes
```
changes where hash_change:(T) show item_path, hash_old, hash_new
```

### Failed Scans
```
scans where state:(E) show root_path, error, started_at
```

## Query Syntax

The Explore interface uses the same query language as the CLI. See [Query Syntax](../query.md) for complete documentation.

## Features

### Column Management

- Show/hide columns using the `show` clause
- Reorder columns with drag-and-drop (if supported)
- Auto-sized columns for readability

### Filtering

Combine filters with `and`/`or`:
```
items where size > 100MB and val:(I) and item_path contains "photos"
```

### Sorting

Use `order by`:
```
items where item_type:(F) order by size desc
```

### Result Limits

Limit output rows:
```
items where item_type:(F) limit 50
```

## Use Cases

- **Advanced Analysis**: Complex queries not available in other UI pages
- **Reporting**: Generate custom reports for specific investigations
- **Debugging**: Inspect raw data for troubleshooting
- **Learning**: Understand the data model and available fields

## Tips

- Start with simple queries and build complexity
- Use `show` to display only relevant columns
- Reference the [Query Syntax](../query.md) guide for field names and operators
- Save frequently-used queries for quick access
