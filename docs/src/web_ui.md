# Interface

The FsPulse interface provides a comprehensive visual environment for monitoring your filesystems, managing scans, analyzing trends, and investigating issues.

## Overview

Access the interface by running:

```sh
fspulse
```

By default, the interface is available at **http://127.0.0.1:8080**. You can customize the host and port through configuration or environment variables (see [Configuration](configuration.md)).

## Pages

The web interface includes the following pages:

- **[Tasks](web_ui/tasks.md)** — Home page showing active tasks, upcoming scans, and task history
- **[Scans](web_ui/scans.md)** — Scan history with results and statistics
- **[Monitor](web_ui/monitor.md)** — Configure scan roots and automatic schedules
- **[Browse](web_ui/browse.md)** — Navigate filesystem hierarchy with tree, folder, and search views
- **[Alerts](web_ui/alerts.md)** — Manage integrity issues and validation failures
- **[Insights](web_ui/insights.md)** — Visualize historical data with interactive charts
- **[Explore](web_ui/explore.md)** — Query interface for advanced data analysis
- **[Settings](web_ui/settings.md)** — Application configuration, database management, and version info

## Live Updates

The web interface uses WebSocket connections to provide real-time updates during task execution. When a scan is running (whether manually initiated or scheduled), you can watch progress updates, statistics, and phase transitions as they happen on the Tasks page.

## Navigation

The left sidebar provides icon-based access to all pages. The sidebar expands on hover to show page labels.
