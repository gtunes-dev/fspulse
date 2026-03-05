# Interface

The fsPulse interface provides a comprehensive visual environment for monitoring your filesystems, managing scans, analyzing trends, and investigating issues.

## Overview

Access the interface by running:

```sh
fspulse
```

By default, the interface is available at **http://127.0.0.1:8080**. You can customize the host and port through configuration or environment variables (see [Configuration](configuration.md)).

## Navigation

The left sidebar organizes pages into two groups:

**Primary** — the pages you'll use most often:

- **[Home](web_ui/home.md)** — Health overview showing root status, active tasks, and recent activity
- **[Browse](web_ui/browse.md)** — Navigate filesystem hierarchy with tree, folder, and search views
- **[Alerts](web_ui/alerts.md)** — Manage integrity issues and validation failures
- **[Trends](web_ui/trends.md)** — Visualize historical data with interactive charts

**Utility** — operational and investigative pages:

- **[History](web_ui/history.md)** — Scan and task activity log
- **Roots** — Add, remove, and scan monitored directories
- **Schedules** — Create and manage automated scan schedules
- **[Data Explorer](web_ui/data_explorer.md)** — Query interface for advanced data analysis
- **[Settings](web_ui/setup.md)** — Edit configuration, view database stats and system info

## Sidebar

The sidebar can be collapsed to icon-only mode for more screen space:
- Click the collapse button in the sidebar footer
- Use the keyboard shortcut **Cmd+B** (macOS) or **Ctrl+B** (Windows/Linux)
- Click the sidebar's right edge (rail) to toggle
- The sidebar automatically collapses on narrower screens and expands on wider ones

When collapsed, hovering over an icon shows a tooltip with the page name.

## Shared Root Context

When you select a root on Browse, Alerts, Trends, or Schedules, that selection is carried across pages via a URL parameter (`?root_id=N`). Selecting a root on Browse and then navigating to Alerts automatically pre-selects the same root, so you don't need to re-select it on every page.

## Live Updates

The web interface uses WebSocket connections to provide real-time updates during task execution. When a scan is running, you can watch progress updates, statistics, and phase transitions as they happen. The sidebar footer also shows a compact progress indicator for the active task.
