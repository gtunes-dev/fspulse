# Home

The Home page is the landing page of fsPulse. It answers the most important question at a glance: **"Is my data safe?"**

<!-- Screenshot: Home page showing root health cards, active task, and upcoming tasks -->
<!-- ![Home Overview](screenshot-placeholder-home-overview.png) -->

---

## Root Health Summary

The centerpiece of the Home page is the root health summary, which shows the status of every monitored directory:

- **Root path** — The directory being monitored
- **Last scan time** — When the root was last scanned, with staleness indicators for roots that haven't been scanned recently
- **Open alerts** — Count of unresolved alerts (highlighted when non-zero)
- **Last scan outcome** — Whether the most recent scan completed successfully, stopped, or errored

If all roots show recent scans with zero alerts, you know your data is healthy. If a root shows open alerts, you know to investigate further.

Each root row is clickable — clicking navigates to the [Browse](browse.md) page for that root.

When no roots have been configured, the Home page shows a welcome message with a link to the [Roots](../web_ui.md) page to add your first root.

<!-- Screenshot: Root health summary showing a mix of healthy and alerted roots -->
<!-- ![Root Health Summary](screenshot-placeholder-health-summary.png) -->

---

## Active Task

When a scan or other task is running, the Home page displays a progress card showing:

- **Task type** and target root
- **Current phase** (Scanning, Sweeping, Analyzing Files, Analyzing Scan)
- **Real-time statistics** — files and folders processed, items hashed, items validated
- **Progress indicator**

Progress updates are delivered in real time via WebSocket. When no task is running, the card shows an idle state with a button to initiate a scan.

---

## Upcoming Tasks

A table shows tasks queued for execution, typically generated from schedules:

- Task type and target root
- Scheduled run time
- Source schedule

Tasks execute sequentially — only one task runs at a time.

---

## Recent Activity

A compact summary of recent scan and task activity, showing the last several completed operations with their outcomes. For the full activity log, click the **View All** link to navigate to the [History](history.md) page.

---

## Scan Now

Click the scan button on the active task card to initiate a scan. You'll select:

- Which root to scan
- Hashing mode (None, Hash changed items, Hash all items)
- Validation mode (None, Validate changed items, Validate all items)

---

## Pause / Resume

fsPulse supports pausing all task execution. When paused:

- A banner appears showing how long tasks have been paused and when the pause will expire (if a duration was set)
- No new tasks will start until resumed
- You can edit the pause duration or resume immediately

This is useful when you want to temporarily prevent scans from running (for example, during a backup window or heavy system load).
