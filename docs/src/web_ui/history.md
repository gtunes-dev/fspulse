# History

The History page provides a complete log of scan and task activity — answering questions like "Did my scheduled scan run?" and "What happened with that scan that errored?"

<!-- Screenshot: History page showing scan/task activity with filters -->
<!-- ![History Page](screenshot-placeholder-history.png) -->

---

## Activity Log

The History page shows a paginated table of completed scans and tasks with key information:

- **Scan ID** and associated root
- **Start and end times**
- **State** (Completed, Stopped, Error)
- **Item counts** — files, folders, and total size discovered
- **Change counts** — additions, modifications, and deletions detected
- **Alert count** — validation failures and suspect hash changes generated
- **Scan options** — whether hashing and validation were enabled

---

## Filtering

Filter the activity log by:
- **Root** — Show activity for a specific monitored directory
- **Time range** — Narrow to a date range
- **Outcome** — Filter by completed, stopped, or errored

---

## Use Cases

- **Schedule verification**: Confirm that scheduled scans ran as expected
- **Troubleshooting**: Identify scans that stopped or errored, and review their details
- **Trend awareness**: Notice patterns in change counts or alert frequency across scans
- **Audit trail**: Review what the system has been doing over any time period

> **Tip**: The [Home](home.md) page shows a compact summary of recent activity. Use the History page when you need the full, filterable log.
