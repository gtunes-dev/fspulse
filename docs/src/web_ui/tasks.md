# Tasks

The Tasks page is the home page of FsPulse. It provides a central view of what FsPulse is doing and what's coming up next.

---

## Active Task

The top of the page shows the current task status:

- **Running task**: Displays the task type (e.g., scanning a root), current phase, and real-time progress statistics including files and folders processed
- **Idle**: When no task is running, shows an idle indicator with a button to initiate a manual scan

Progress updates are delivered in real time via WebSocket, so you can watch scanning phases (Scanning, Sweeping, Analyzing) progress live.

---

## Upcoming Tasks

Below the active task, a table shows tasks queued for execution. These are typically generated from schedules and include:

- Task type and target root
- Scheduled run time
- Source (which schedule created the task)

Tasks execute sequentially — only one task runs at a time.

---

## Task History

A paginated table of completed tasks shows:

- Task type and target root
- Start and completion times
- Final status (Completed, Stopped, Error)

---

## Manual Scans

Click the scan button on the active task card to initiate a manual scan. You'll select:

- Which root to scan
- Hashing mode (None, Hash changed items, Hash all items)
- Validation mode (None, Validate changed items, Validate all items)

---

## Pause / Resume

FsPulse supports pausing all task execution. When paused:

- A banner appears at the top of the page showing how long tasks have been paused and when the pause will expire (if a duration was set)
- No new tasks will start until resumed
- You can edit the pause duration or resume immediately

This is useful when you want to temporarily prevent scans from running (for example, during a backup window or heavy system load).
