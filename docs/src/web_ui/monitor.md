# Monitor & Scheduling

The Monitor page is your control center for managing scan roots, scheduling automatic scans, and viewing the scan queue.

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-monitor-schedules.png" alt="Monitor Page" style="width: 90%; max-width: 900px;">
</p>

## Managing Scan Roots

### Adding a Root

1. Click **Add Root**
2. Enter the full filesystem path to monitor
3. Optionally provide a friendly name
4. Save

### Managing Roots

- **Scan Now**: Trigger an immediate one-time scan
- **Delete**: Remove the root (also removes associated schedules and queue entries)
- View root statistics and last scan time

## Scheduling Automatic Scans

FsPulse supports flexible scheduling options for automated monitoring:

### Schedule Types

- **Daily**: Run at a specific time each day
- **Weekly**: Run on specific days of the week at a chosen time
- **Monthly**: Run on a specific day of the month
- **Interval**: Run every N hours/minutes

### Creating a Schedule

1. Click **Add Schedule**
2. Select the root to scan
3. Choose schedule type and timing
4. Configure scan options:
   - Enable hashing (default: all files)
   - Enable validation (default: new/changed files)
5. Save

### Schedule Management

- **Enable/Disable**: Temporarily pause schedules without deleting them
- **Edit**: Modify timing or scan options
- **Delete**: Remove the schedule

## Scan Queue

The queue shows:
- Pending scheduled scans waiting to execute
- Currently running scans
- Recent scan history

Scans are queued and executed sequentially to prevent resource conflicts.

## Configuration

Scheduling and queue behavior can be customized via [Configuration](../configuration.md).
