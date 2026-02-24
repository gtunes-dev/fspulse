# Monitor

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

- **Scan Now**: Create a manual scan task for the root
- **Delete**: Remove the root (also removes associated schedules)
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

Scans and other tasks are queued and executed sequentially to prevent resource conflicts. You can view upcoming and running tasks on the [Tasks](tasks.md) page.
