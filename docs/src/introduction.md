# Introduction

<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" style="width: 100%; max-width: 600px;">

## What is FsPulse?

**FsPulse is an essential filesystem integrity tool for system administrators, home-lab enthusiasts, and anyone who takes data preservation seriously.** It runs as a background service that continuously monitors your critical directories, watching for the silent threats that traditional backup systems miss: **bit rot, corruption, and unexpected tampering**.

Your files can change without you knowing. Hard drives degrade. Ransomware alters files while preserving timestamps. FsPulse catches these problems early through two powerful detection methods:

- **Content Hashing (SHA2)**: Detects when file contents change even though filesystem metadata stays the same—the telltale sign of bit rot or sophisticated tampering
- **Format Validation**: Uses open-source libraries to read and validate file structures, catching corruption in FLAC audio, JPEG/PNG images, PDFs, and more

Instead of waiting for a file to fail when you need it most, FsPulse gives you **continuous awareness**. Run manual scans when you want, or let scheduled scans (coming soon) monitor your data automatically. When issues are detected, FsPulse's alert system flags them immediately through an elegant web interface.

Whether you're protecting family photos, managing media libraries, or maintaining production servers, FsPulse provides the peace of mind that comes from knowing your data is actually intact—not just backed up.

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-scan-progress.png" alt="FsPulse Web UI - Real-time Scan Monitoring" style="width: 90%; max-width: 900px;">
  <br>
  <em>Web UI showing real-time scan progress with live statistics</em>
</p>

## Key Capabilities

- **Dual Interface** — Run as a web service with elegant browser UI, or use the full-featured CLI with interactive terminal modes
- **Integrity Detection** — SHA2 hashing catches content changes even when filesystem metadata stays the same; format validators detect corruption in supported file types
- **Change Tracking** — Deep directory scanning captures all additions, modifications, and deletions across scan sessions
- **Alert System** — Suspicious hash changes and validation failures are flagged immediately with status management (Open/Flagged/Dismissed)
- **Powerful Query Language** — SQL-inspired syntax lets you filter, sort, and analyze your data with precision
- **Production Ready** — Official Docker images (multi-architecture), comprehensive documentation, and native installers

## Usage Modes

FsPulse offers flexibility in how you interact with it:

- **Web UI Mode**: Run `fspulse serve` to start the server and access the full web interface at http://127.0.0.1:8080. Great for visual data exploration, managing multiple roots, and real-time scan monitoring.

- **Command-Line Mode**: Use direct terminal commands like `fspulse scan`, `fspulse query`, and `fspulse report` for automation, scripted workflows, and quick one-off operations.

- **Interactive Terminal Mode**: Use `fspulse interact` for menu-driven navigation or `fspulse explore` for a full-screen data explorer TUI—perfect for terminal users who want visual feedback without leaving the command line.

FsPulse is designed to scale across large file systems while maintaining clarity and control for the user.

---

This book provides comprehensive documentation for all aspects of FsPulse. Start with [Getting Started](getting_started.md) for installation, or jump to any section that interests you.
