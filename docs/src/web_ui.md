# Interface

The FsPulse interface provides a comprehensive visual environment for monitoring your filesystems, managing scans, analyzing trends, and investigating issues.

## Overview

Access the interface by running:

```sh
fspulse
```

By default, the interface is available at **http://127.0.0.1:8080**. You can customize the host and port through configuration or environment variables (see [Configuration](configuration.md)).

## Key Features

The web interface includes the following pages:

- **[Scans](web_ui/scans.md)** — Dashboard showing scan status and history
- **[Monitor](web_ui/monitor.md)** — Configure automatic scans and manage scan roots
- **[Browse](web_ui/browse.md)** — Navigate filesystem hierarchy with detailed item inspection
- **[Alerts](web_ui/alerts.md)** — Manage integrity issues and validation failures
- **[Insights](web_ui/insights.md)** — Visualize historical data with interactive charts
- **[Explore](web_ui/explore.md)** — Query interface for advanced data analysis

## Live Updates

The web interface uses WebSocket connections to provide real-time updates during scan operations. When a scan is running (whether manually initiated or scheduled), you can watch progress updates, statistics, and phase transitions as they happen.

## Navigation

The left sidebar provides access to all major sections. On smaller screens, the sidebar collapses into a hamburger menu for better mobile experience.
