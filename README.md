<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" width="100%" style="max-width: 600px;">

# FsPulse

> **⚠️ Early Development Notice**
> FsPulse is under active development and is not yet feature-complete. Core functionality is stable, but expect continued evolution and occasional breaking changes. Feedback and issue reports are welcome as we work toward a 1.0 release.

[![Docker Pulls](https://img.shields.io/docker/pulls/gtunesdev/fspulse)](https://hub.docker.com/r/gtunesdev/fspulse)
[![GitHub release](https://img.shields.io/github/v/release/gtunes-dev/fspulse)](https://github.com/gtunes-dev/fspulse/releases)

---

## What is FsPulse?

**FsPulse is an essential filesystem integrity tool for system administrators, home-lab enthusiasts, and anyone who takes data preservation seriously.** It runs as a background service that continuously monitors your critical directories, watching for the silent threats that traditional backup systems miss: **bit rot, corruption, and unexpected tampering**.

Your files can change without you knowing. Hard drives degrade. Ransomware alters files while preserving timestamps. FsPulse catches these problems early through two powerful detection methods:

- **Content Hashing (SHA2)**: Detects when file contents change even though filesystem metadata stays the same—the telltale sign of bit rot or sophisticated tampering
- **Format Validation**: Uses open-source libraries to read and validate file structures, catching corruption in FLAC audio, JPEG/PNG images, PDFs, and more

Instead of waiting for a file to fail when you need it most, FsPulse gives you **continuous awareness**. Run manual scans when you want, or let scheduled scans (coming soon) monitor your data automatically. When issues are detected, FsPulse's alert system flags them immediately through an elegant web interface.

Whether you're protecting family photos, managing media libraries, or maintaining production servers, FsPulse provides the peace of mind that comes from knowing your data is actually intact—not just backed up.

---

## 🚀 Key Capabilities

- **Dual Interface** — Run as a web service with elegant browser UI, or use the full-featured CLI with interactive terminal modes
- **Integrity Detection** — SHA2 hashing catches content changes even when filesystem metadata stays the same; format validators detect corruption in supported file types
- **Change Tracking** — Deep directory scanning captures all additions, modifications, and deletions across scan sessions
- **Alert System** — Suspicious hash changes and validation failures are flagged immediately with status management (Open/Flagged/Dismissed)
- **Powerful Query Language** — SQL-inspired syntax lets you filter, sort, and analyze your data with precision
- **Production Ready** — Official Docker images (multi-architecture), comprehensive documentation, and native installers

---

## 📚 Documentation

Quick start instructions are below, but full documentation is available in book form:

👉 **[FsPulse Documentation](https://gtunes-dev.github.io/fspulse/)**

Key sections:
- [Getting Started](https://gtunes-dev.github.io/fspulse/getting_started.html) — Installation options and first steps
- [Docker Deployment](https://gtunes-dev.github.io/fspulse/docker.html) — Complete Docker guide with NAS setup
- [Scanning](https://gtunes-dev.github.io/fspulse/scanning.html) — How scans work and what they detect
- [Query Syntax](https://gtunes-dev.github.io/fspulse/query.html) — Powerful filtering and data exploration
- [Command-Line Interface](https://gtunes-dev.github.io/fspulse/cli.html) — All CLI commands and options
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
# Run a scan
fspulse scan --root-path /path/to/files --hash --validate

# Query for invalid items
fspulse query "items where val:(I)"

# View recent scans
fspulse report scans --last 5

# Find items with hash changes
fspulse query "changes where hash_change:(T) show item_path, hash_old, hash_new"
```

**Great for:** Automation, scripted workflows, CI/CD integration, quick one-off operations

---

### Interactive Terminal Mode

Menu-driven interfaces for guided terminal workflows:

```sh
fspulse interact  # Menu-driven interface
fspulse explore   # Full-screen data explorer
```

**Great for:** Terminal users who want visual feedback without leaving the command line

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

