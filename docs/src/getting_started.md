# Getting Started

FsPulse can be installed in one of four ways:

1. **Run with Docker (Recommended)**
2. **Install via [crates.io](https://crates.io/crates/fspulse)**
3. **Clone and build from source**
4. **Download a pre-built release binary from GitHub**

Choose the method that works best for your platform and preferences.

---

## 1. Run with Docker (Recommended)

The easiest way to run FsPulse is with Docker:

```sh
docker pull gtunesdev/fspulse:latest

docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -v fspulse-data:/data \
  gtunesdev/fspulse:latest
```

Access the web UI at **http://localhost:8080**

The web UI provides full functionality: managing roots, initiating scans, querying data, and viewing results—all from your browser.

See the [Docker Deployment](docker.md) chapter for complete documentation including:
- Volume management for scanning host directories
- Configuration options
- Docker Compose examples
- NAS deployment (TrueNAS, Unraid)
- Troubleshooting

---

## 2. Install via Crates.io

The easiest way to get FsPulse is via [crates.io](https://crates.io/crates/fspulse):

```sh
cargo install fspulse
```

This will download, compile, and install the latest version of FsPulse into Cargo’s `bin` directory, typically `~/.cargo/bin`. That directory is usually already in your `PATH`. If it's not, you may need to add it manually.

Then run:

```sh
fspulse --help
```

To upgrade to the latest version later:

```sh
cargo install fspulse --force
```

---

## 3. Clone and Build from Source

If you prefer working directly with the source code (for example, to contribute or try out development versions):

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
```

Then run it from the release build directory:

```sh
./target/release/fspulse --help
```

---

## 4. Download Pre-Built Release Binaries

Pre-built release binaries for Linux, macOS, and Windows are available on the [GitHub Releases page](https://github.com/gtunes-dev/fspulse/releases):

1. Visit the releases page.
2. Download the appropriate archive for your operating system.
3. Unpack the archive.
4. Optionally move the `fspulse` binary to a directory included in your `PATH`.

For example, on Unix systems:

```sh
mv fspulse /usr/local/bin/
```

Then confirm it's working:

```sh
fspulse --help
```

---

## Usage: Web UI or CLI

After installation, you can use FsPulse in two ways:

### Web UI (Server Mode)

Start the server:

```sh
fspulse serve
```

Then open your browser to **http://localhost:8080** to access the web interface.

The web UI provides:
- Root management (create, view, delete roots)
- Scan initiation with real-time progress
- Interactive data exploration
- Powerful query interface

### Command-Line Interface

Use FsPulse directly from the terminal for data analysis:

**Interactive exploration:**
```sh
fspulse interact  # Menu-driven interface
fspulse explore   # Data explorer TUI
```

**Query and report on scan results:**
```sh
# Items whose path contains 'reports'
fspulse query "items where item_path:('reports')"

# Changes involving items detected as invalid
fspulse query "changes where val_new:(I) show default, val_old, val_new order by change_id desc"

# View recent scans
fspulse report scans --last 5
```

> **Note:** Scanning is performed through the web UI. The CLI provides powerful tools for querying and analyzing scan results.

See the [Query Syntax](query.md) page for more examples and the [Command-Line Interface](cli.md) page for all available commands.

