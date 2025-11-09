# Introduction

<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" style="width: 100%; max-width: 600px;">

## What is FsPulse?

**FsPulse is a comprehensive filesystem monitoring and integrity tool that gives you complete visibility into your critical directories. Track your data as it grows and changes over time, detect unexpected modifications, and catch silent threats like bit rot and corruption before they become disasters. FsPulse provides continuous awareness through automated scanning, historical trend analysis, and intelligent alerting.**

Your filesystem is constantly evolving—files are added, modified, and deleted. Storage grows. But **invisible problems** hide beneath the surface: bit rot silently corrupts data, ransomware alters files while preserving timestamps, and you don't realize directories have bloated.

FsPulse gives you **continuous awareness** of both the visible and invisible:

**Monitor Change & Growth:**
- Track directory sizes and growth trends over time
- Visualize file additions, modifications, and deletions
- Understand what's changing and when across all scans

**Detect Integrity Issues:**
- **Content Hashing (SHA2)**: Catches when file contents change even though metadata stays the same—the signature of bit rot or tampering
- **Format Validation**: Reads and validates file structures to detect corruption in FLAC, JPEG, PNG, PDF, and more

Whether you're managing storage capacity, tracking project evolution, or ensuring data integrity, FsPulse provides the visibility and peace of mind that comes from truly knowing the state of your data.

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-scan-progress.png" alt="FsPulse Web UI - Real-time Scan Monitoring" style="width: 90%; max-width: 900px;">
  <br>
  <em>Web UI showing real-time scan progress with live statistics</em>
</p>

## Key Capabilities

- **Continuous Monitoring** — Schedule recurring scans (daily, weekly, monthly, or custom intervals) to track your filesystem automatically
- **Size & Growth Tracking** — Monitor directory sizes and visualize storage trends over time with dual-format units
- **Change Detection** — Track all file additions, modifications, and deletions with complete historical records
- **Integrity Verification** — SHA2 hashing detects bit rot and tampering; format validators catch corruption in supported file types
- **Historical Analysis** — Interactive trend charts show how your data evolves: sizes, counts, changes, and alerts
- **Alert System** — Suspicious hash changes and validation failures flagged immediately with status management
- **Powerful Query Language** — SQL-inspired syntax for filtering, sorting, and analyzing your filesystem data
- **Dual Interface** — Elegant web UI for visual exploration, full-featured CLI for automation and scripting

## Usage Modes

FsPulse offers flexibility in how you interact with it:

- **Web UI Mode**: Run `fspulse serve` to start the server and access the full web interface at http://127.0.0.1:8080. Great for visual data exploration, managing multiple roots, and real-time scan monitoring.

- **Command-Line Mode**: Use direct terminal commands like `fspulse query` and `fspulse report` for automation, scripted workflows, and quick data analysis operations.

- **Interactive Terminal Mode**: Use `fspulse interact` for menu-driven navigation or `fspulse explore` for a full-screen data explorer TUI—perfect for terminal users who want visual feedback without leaving the command line.

FsPulse is designed to scale across large file systems while maintaining clarity and control for the user.

---

This book provides comprehensive documentation for all aspects of FsPulse. Start with [Getting Started](getting_started.md) for installation, or jump to any section that interests you.
