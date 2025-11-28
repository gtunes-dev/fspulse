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

## Running FsPulse

After installation, start the FsPulse server:

```sh
fspulse
```

Or explicitly:

```sh
fspulse serve
```

Then open your browser to **http://localhost:8080** to access the web interface.

FsPulse is a **web-first application**. All functionality is available through the web UI:
- Root management (create, view, delete roots)
- Scan scheduling and initiation with real-time progress
- Interactive data browsing and exploration
- Powerful query interface
- Alert management

### Configuration

FsPulse is configured through environment variables or a config file, not command-line flags:

```sh
# Example: Change port and enable debug logging
export FSPULSE_SERVER_PORT=9090
export FSPULSE_LOGGING_FSPULSE=debug
fspulse
```

See [Configuration](configuration.md) for all available settings and the [Command-Line Interface](cli.md) page for more details.

