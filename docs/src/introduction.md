<p align="center">
  <img src="images/icon-256.png" alt="fsPulse" width="128">
  <br>
  <strong style="font-size: 1.5em; color: #14B8A6; font-family: sans-serif;">fsPulse</strong>
</p>

# Introduction

> **Read-Only Guarantee.**
> fsPulse **never modifies your files**. It requires only read access to the directories you configure for scanning. Write access is required only for fsPulse's own database, configuration files, and logs — never for your data.

> **Local-Only Guarantee.**
> fsPulse makes no outbound network requests. All functionality runs entirely on your local system, with no external dependencies or telemetry.

## What is fsPulse?

**fsPulse is a comprehensive filesystem monitoring and integrity tool that gives you complete visibility into your critical directories. Track your data as it grows and changes over time, detect unexpected modifications, and catch silent threats like bit rot and corruption before they become disasters. fsPulse provides continuous awareness through automated scanning, historical trend analysis, and a dedicated integrity view for reviewing issues.**

Your filesystem is constantly evolving—files are added, modified, and deleted. Storage grows. But **invisible problems** hide beneath the surface: bit rot silently corrupts data, ransomware alters files while preserving timestamps, and you don't realize directories have bloated.

fsPulse gives you **continuous awareness** of both the visible and invisible:

**Monitor Change & Growth:**
- Track directory sizes and growth trends over time
- Visualize file additions, modifications, and deletions
- Understand what's changing and when across all scans

**Detect Integrity Issues:**
- **Content Hashing (SHA2)**: Catches when file contents change even though metadata stays the same—the signature of bit rot or tampering
- **Format Validation**: Reads and validates file structures to detect corruption in FLAC, JPEG, PNG, PDF, and more

Whether you're managing storage capacity, tracking project evolution, or ensuring data integrity, fsPulse provides the visibility and peace of mind that comes from truly knowing the state of your data.

<!-- Screenshot: Home page showing root health summary with last scan times -->
<!-- ![fsPulse Home](screenshot-placeholder-home.png) -->

## Key Capabilities

- **Health-at-a-Glance Overview** — See the status of all monitored directories immediately: integrity issues, last scan times, and overall health
- **Continuous Monitoring** — Schedule recurring scans (daily, weekly, monthly, or custom intervals) to track your filesystem automatically
- **Temporal Versioning** — Every item's state is tracked over time; browse your filesystem as it appeared at any past scan
- **Size & Growth Tracking** — Monitor directory sizes and visualize storage trends over time with dual-format units
- **Change Detection** — Track all file additions, modifications, and deletions through version history
- **Integrity Verification** — SHA2 hashing detects bit rot and tampering; format validators catch corruption in supported file types
- **Integrity Management** — Dedicated Integrity page surfaces suspect hashes and validation failures with review tracking
- **Historical Trends** — Interactive trend charts show how your data evolves: sizes, counts, changes, and integrity metrics
- **Powerful Query Language** — SQL-inspired syntax for filtering, sorting, and analyzing across four data domains
- **Web-First Design** — Elegant web UI for all operations including scanning, browsing, querying, and configuration

## Running fsPulse

fsPulse is a **web-first application**. Start the server and access all functionality through your browser:

```sh
fspulse
```

Then open **http://localhost:8080** in your browser to access the web interface.

The web UI provides complete functionality for managing roots, scheduling and monitoring scans, browsing your filesystem data, running queries, and reviewing integrity issues. Configuration is done through environment variables or a config file—see [Configuration](configuration.md) for details.

fsPulse is designed to scale across large file systems while maintaining clarity and control for the user.

---

This book provides comprehensive documentation for all aspects of fsPulse. Start with [Getting Started](getting_started.md) for installation, or jump to any section that interests you.
