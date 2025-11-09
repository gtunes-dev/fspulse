# Report Commands

FsPulse provides CLI commands for generating formatted reports about scans, roots, and system status.

## Available Reports

### Scans Report

View scan history and details:

```sh
fspulse report scans
```

Options:
- `--last N`: Show only the N most recent scans
- `--root-id ID`: Filter to specific root
- `--state STATE`: Filter by scan state (Completed, Error, Stopped)

Example:

```sh
# Last 5 scans
fspulse report scans --last 5

# Failed scans only
fspulse report scans --state Error
```

### Roots Report

List configured scan roots:

```sh
fspulse report roots
```

Shows:
- Root ID and path
- Friendly name (if set)
- Last scan time
- Scan count

### Scan Details

Get detailed information about a specific scan:

```sh
fspulse report scan-detail --scan-id <ID>
```

Shows:
- Scan metadata (start/end time, duration, state)
- Statistics (files/folders scanned, sizes, changes)
- Change breakdown (adds, modifies, deletes)
- Alert summary
- Error details (if scan failed)

## Output Format

Reports are formatted as tables with:
- Clear headers
- Aligned columns
- Color-coded status indicators (where supported)

## Use Cases

- **Quick Status**: Check recent scan activity
- **Debugging**: Investigate failed scans
- **Auditing**: Review scan history
- **Monitoring**: Script periodic status checks

## Example Workflows

### Check for Failed Scans

```sh
fspulse report scans --state Error
```

### Review Last Scan Results

```sh
# Get the last scan ID
LAST_SCAN=$(fspulse report scans --last 1 | grep -oE 'ID: [0-9]+' | cut -d' ' -f2)

# Show details
fspulse report scan-detail --scan-id $LAST_SCAN
```

### List All Configured Roots

```sh
fspulse report roots
```

## Integration with Queries

For more complex analysis, use the [Query Command](query.md):

```sh
# Custom scan query
fspulse query "scans where duration_secs > 3600 show root_path, duration_secs, started_at"
```
