# Query Command

The `fspulse query` command provides powerful data analysis capabilities from the command line.

## Basic Usage

```sh
fspulse query "<query_string>"
```

## Query Syntax

Queries use an SQL-inspired syntax. See [Query Syntax](../query.md) for complete documentation.

## Examples

### Find Invalid Items

```sh
fspulse query "items where val:(I)"
```

### Large Directories

```sh
fspulse query "items where size > 10GB and item_type:(D)"
```

### Recent Hash Changes

```sh
fspulse query "changes where hash_change:(T) show item_path, hash_old, hash_new"
```

### Scan History

```sh
fspulse query "scans order by started_at desc limit 10"
```

## Output Format

Results are displayed as formatted tables in the terminal using:
- Column alignment
- Header rows
- Truncation for long values

## Common Patterns

### Filtering by Validation Status

```sh
# Invalid items only
fspulse query "items where val:(I)"

# Valid items only
fspulse query "items where val:(V)"

# Not validated
fspulse query "items where val:(N)"
```

### Size Queries

```sh
# Files over 1GB
fspulse query "items where size > 1GB and item_type:(F)"

# Total size of directories
fspulse query "items where item_type:(D) show item_path, size order by size desc"
```

### Change Tracking

```sh
# All additions in last scan
fspulse query "changes where change:(A)"

# Modifications with hash changes
fspulse query "changes where change:(M) and hash_change:(T)"

# Deletions
fspulse query "changes where change:(D)"
```

## Integration

The query command is ideal for:
- **Scripting**: Automate analysis workflows
- **CI/CD**: Integrate with build pipelines
- **Monitoring**: Generate alerts from query results
- **Reporting**: Export data for external tools

## Interactive Mode

For an interactive query experience, use:

```sh
fspulse interact
```

Or for a full-screen TUI:

```sh
fspulse explore
```

See [Interactive Mode](../interactive_mode.md) for details.
