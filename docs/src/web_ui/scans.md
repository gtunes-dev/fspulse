# Scans

The Scans page serves as your central dashboard, providing an at-a-glance view of scan status and history.

## Features

### Active Scan Monitoring

When a scan is running (manually initiated or scheduled), the Scans page displays:
- Real-time progress indicators
- Current scan phase (Discovery, Hash, Validation, Analysis)
- Statistics: files/folders processed, sizes calculated, changes detected
- Live updates via WebSocket connection

### Recent Scan Results

For completed scans, the Scans page shows:
- Scan completion time
- Total items scanned
- Change summary (additions, modifications, deletions)
- Alert count (validation failures, suspicious hash changes)
- Storage metrics

### Quick Access

From the Home page, you can quickly navigate to:
- Browse the scanned filesystem
- View detailed scan reports
- Investigate alerts
- Configure new scans

## Use Cases

- **Monitoring**: Keep the Home page open to watch long-running scans
- **Status Check**: Quick view of your most recent scan activity
- **Alert Awareness**: Immediate visibility into any detected issues
