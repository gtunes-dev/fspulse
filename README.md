<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" width="100%" style="max-width: 600px;">

# FsPulse

> **⚠️ Early Development Notice**
> FsPulse is under active development and is not yet feature-complete. Core functionality is stable, but expect continued evolution and occasional breaking changes. Feedback and issue reports are welcome as we work toward a 1.0 release.

[![Docker Pulls](https://img.shields.io/docker/pulls/gtunesdev/fspulse)](https://hub.docker.com/r/gtunesdev/fspulse)
[![GitHub release](https://img.shields.io/github/v/release/gtunes-dev/fspulse)](https://github.com/gtunes-dev/fspulse/releases)

---

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
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-scan-progress.png" alt="FsPulse Web UI - Real-time Scan Monitoring" width="90%" style="max-width: 900px;">
  <br>
  <em>Web UI showing real-time scan progress with live statistics</em>
</p>

---

## 🚀 Key Capabilities

- **Continuous Monitoring** — Schedule recurring scans (daily, weekly, monthly, or custom intervals) to track your filesystem automatically
- **Size & Growth Tracking** — Monitor directory sizes and visualize storage trends over time with dual-format units
- **Change Detection** — Track all file additions, modifications, and deletions with complete historical records
- **Integrity Verification** — SHA2 hashing detects bit rot and tampering; format validators catch corruption in supported file types
- **Historical Analysis** — Interactive trend charts show how your data evolves: sizes, counts, changes, and alerts
- **Alert System** — Suspicious hash changes and validation failures flagged immediately with status management
- **Powerful Query Language** — SQL-inspired syntax for filtering, sorting, and analyzing your filesystem data
- **Dual Interface** — Elegant web UI for visual exploration, full-featured CLI for automation and scripting

---

## 📚 Documentation

Quick start instructions are below, but full documentation is available in book form:

👉 **[FsPulse Documentation](https://gtunes-dev.github.io/fspulse/)**

Key sections:
- [Getting Started](https://gtunes-dev.github.io/fspulse/getting_started.html) — Installation, Docker deployment, and first steps
- [Web Interface](https://gtunes-dev.github.io/fspulse/web_ui.html) — Complete guide to Monitor, Browse, Insights, Alerts, and Explore pages
- [Scanning Concepts](https://gtunes-dev.github.io/fspulse/scanning.html) — How scans work, hashing, and validation
- [Query Syntax](https://gtunes-dev.github.io/fspulse/query.html) — Powerful filtering and data exploration
- [Command-Line Interface](https://gtunes-dev.github.io/fspulse/cli.html) — CLI commands for automation and scripting
- [Configuration](https://gtunes-dev.github.io/fspulse/configuration.html) — Customizing FsPulse behavior

---

## 🐳 Quick Start with Docker

```sh
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -v fspulse-data:/data \
  -v ~/Documents:/roots/documents:ro \
  gtunesdev/fspulse:latest
```

Access the web UI at **http://localhost:8080**

The [Docker Deployment Guide](https://gtunes-dev.github.io/fspulse/docker.html) provides complete coverage including Docker Compose examples, NAS deployments, and detailed configuration options.

---

## ⚡ Usage Examples

FsPulse can run in three modes depending on your needs:

### Web UI Mode

Start the server and access through your browser:

```sh
fspulse serve
```

Open **http://127.0.0.1:8080** in your browser to access the full web interface.

**Great for:** Visual data exploration, managing multiple roots, real-time scan monitoring, continuous awareness

---

### Command-Line Mode

Direct terminal commands for scripting and automation:

```sh
# Query for invalid items
fspulse query "items where val:(I)"

# View recent scans
fspulse report scans --last 5

# Find items with hash changes
fspulse query "changes where hash_change:(T) show item_path, hash_old, hash_new"

# Find directories over 10GB
fspulse query "items where size > 10GB and item_type:(D)"
```

**Great for:** Automation, scripted workflows, CI/CD integration, quick one-off operations

**Note:** All scanning is performed through the web UI. The CLI provides powerful querying and reporting capabilities.

---

### Interactive Terminal Mode

Menu-driven interfaces for guided terminal workflows:

```sh
fspulse interact  # Menu-driven interface
fspulse explore   # Full-screen data explorer
```

**Great for:** Terminal users who want visual feedback without leaving the command line

---

### 🖥️ Web Interface Highlights

The web UI provides powerful visual tools for monitoring and exploring your data:

- **Monitor & Schedule** — Configure automatic scans with flexible scheduling options, view execution queue status, and manage scan roots
- **Live Scan Progress** — Watch scan activity in real-time whether manually initiated or scheduled, with detailed phase-by-phase statistics
- **Browse with Detail View** — Explore your filesystem hierarchy with elegant sliding panels showing item metadata, validation status, alerts, and complete change history
- **Insights & Trends** — Interactive charts tracking file sizes, counts, change activity, and validation issues over time with customizable date ranges
- **Alert Management** — Filter, flag, and dismiss integrity issues with context-aware views and status tracking

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-monitor-schedules.png" alt="FsPulse Monitor Page - Scheduled Scans" width="90%" style="max-width: 900px;">
  <br>
  <em>Monitor page showing scheduled scans and queue management</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-browse-tree.png" alt="FsPulse Browse Page - Filesystem Tree" width="90%" style="max-width: 900px;">
  <br>
  <em>Browse page showing filesystem hierarchy navigation</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-browse-detail.png" alt="FsPulse Browse Page - Item Detail Panel" width="90%" style="max-width: 900px;">
  <br>
  <em>Item detail panel showing metadata, validation status, and change history</em>
</p>

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-insights-trends.png" alt="FsPulse Insights - Trend Analysis" width="90%" style="max-width: 900px;">
  <br>
  <em>Insights page with interactive charts for historical trend analysis</em>
</p>

---

## 📦 Installation Options

FsPulse can be installed in several ways depending on your preferences and environment:

### Docker Hub (Recommended)

Pull the official image and run:

```sh
docker pull gtunesdev/fspulse:latest
docker run -d --name fspulse -p 8080:8080 -v fspulse-data:/data gtunesdev/fspulse:latest
```

Multi-architecture support: `linux/amd64`, `linux/arm64`

See the [Docker Deployment Guide](https://gtunes-dev.github.io/fspulse/docker.html) for complete instructions.

### Cargo (crates.io)

Install via Rust's package manager:

```sh
cargo install fspulse
```

Requires Rust toolchain installed on your system.

### Pre-built Binaries

Download platform-specific binaries from [GitHub Releases](https://github.com/gtunes-dev/fspulse/releases).

Available for: Linux, macOS, Windows

macOS builds include both Intel (x86_64) and Apple Silicon (ARM64) binaries.

**Note:** All web UI assets are embedded in the binary—no external files or dependencies required.

### Build from Source

Clone and build with Cargo:

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
./target/release/fspulse --help
```

See the [Getting Started Guide](https://gtunes-dev.github.io/fspulse/getting_started.html) for detailed installation instructions for all methods.

---

## 💬 Getting Help

- **Report issues:** [GitHub Issues](https://github.com/gtunes-dev/fspulse/issues)
- **Documentation:** [FsPulse Book](https://gtunes-dev.github.io/fspulse/)
- **Docker Hub:** [gtunesdev/fspulse](https://hub.docker.com/r/gtunesdev/fspulse)

---

## 🤝 Contributions

FsPulse is under active development, but is **not currently accepting external contributions**. This may change in the future — see our [Development Guide](https://gtunes-dev.github.io/fspulse/development.html) for details.

---

## 📄 License

Released under the MIT License. See [LICENSE](LICENSE) for details.

