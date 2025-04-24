# Getting Started

FsPulse can be installed in one of three ways:

1. **Install via [crates.io](https://crates.io/crates/fspulse)**
2. **Clone and build from source**
3. **Download a pre-built release binary from GitHub**

Choose the method that works best for your platform and preferences.

---

## 1. Install via Crates.io

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

## 2. Clone and Build from Source

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

## 3. Download Pre-Built Release Binaries

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

## First Scan

To scan a directory:

```sh
fspulse scan --root-path /some/directory
```

---

## Interactive Exploration

After scanning, you can explore results in an interactive shell:

```sh
fspulse interact
```

---

## Querying

Use flexible, SQL-like queries to retrieve and filter scan results:

```sh
# Items whose path contains 'reports'
fspulse query "items where item_path:('reports')"

# Changes involving items detected as invalid
fspulse query "changes where val_new:(I) show default, val_old, val_new order by change_id desc"
```

See the [Query Syntax](query.md) page for more examples.

