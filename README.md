<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="banner with image of folder and magnifying glass">

# FsPulse

FsPulse is a command-line tool designed to capture the state of directories and detect changes over time. It records file and directory metadata, tracking additions, deletions, and modifications. FsPulse optionally computes MD5 hashes of file contents to detect changes even if metadata remains unchanged. Additionally, FsPulse can validate file contents by examining and decoding their contents. Validation is currently done with limited, but growing, set of Rust libraries listed below.

## Validators

FsPulse attempts to validate the following file types

- **FLAC** with [**claxon**](https://github.com/ruuda/claxon) (https://github.com/ruuda/claxon)

- **JPG/JPEG, GIF, BMP** with [**image**](https://github.com/image-rs/image) (https://github.com/image-rs/image)

- **PDF** with [**lopdf**](https://github.com/J-F-Liu/lopdf) (https://github.com/J-F-Liu/lopdf)

## Overview

FsPulse organizes information into four structured entities:

- **Roots:**  
  Directories explicitly scanned with FsPulse. Each root corresponds to a unique path on your filesystem and serves as a starting point for scans.

- **Scans:**  
  Snapshots capturing the state of a root directory at a specific point in time. Scans record detailed metadata about every file and directory encountered.

- **Items:**  
  Files or directories identified within a scan. Each item maintains metadata, including path, file size, timestamps, and optionally, content hashes and validation status.

- **Changes:**  
  Modifications detected between consecutive scans, including additions, deletions, and modifications of items within a root.

FsPulse stores scan data in a SQLite database named `fspulse.db`. By default, this database is located in your home directory (`~/` on Unix-based systems, `%USERPROFILE%\` on Windows). You can optionally specify a different database location using the `--db-path` parameter.

## Installation

To install FsPulse, clone the repository and build it with Cargo:

```sh
cargo build --release
```

Move the compiled binary to a location in your `PATH` or run it directly from the build directory.

## Usage

### Interactive Mode

Launch FsPulse in interactive mode:

```sh
fspulse interact
```

Interactive mode provides step-by-step guidance through common tasks.

### Scanning a Directory

To scan a directory (creating a root if needed):

```sh
fspulse scan --root-path /some/directory
```

To scan an existing root by ID:

```sh
fspulse scan --root-id 123
```

To scan the most recently scanned root:

```sh
fspulse scan --last
```

To include content hashing (MD5) during a scan:

```sh
fspulse scan --root-path /some/directory --hash
```

To validate files (currently supports `.flac` files validated using the [`claxon`](https://github.com/ruuda/claxon) crate):

```sh
fspulse scan --root-path /some/directory --validate
```

If a scan does not complete, FsPulse remembers its state. Upon the next scan of the same root, you'll be prompted to either resume or discard the incomplete scan. Only one scan per root can be active at a time.

### Reporting

FsPulse provides detailed reporting options:

#### Roots

Show all roots scanned:

```sh
fspulse report roots
```

Show a specific root by ID or path:

```sh
fspulse report roots --root-id 123
fspulse report roots --root-path /some/directory
```

#### Scans

Show recent scans (default last 10 scans):

```sh
fspulse report scans
```

Show specific scan by ID or a custom number of recent scans:

```sh
fspulse report scans --scan-id 456
fspulse report scans --last 5
```

#### Items

Show a specific item by ID or path:

```sh
fspulse report items --item-id 789
fspulse report items --item-path /some/file
```

Show all items from the most recent scan of a root:

```sh
fspulse report items --root-id 123
```

Show invalid items within a specific root:

```sh
fspulse report items --root-id 123 --invalid
```

#### Changes

Show all changes from a specific scan:

```sh
fspulse report changes --scan-id 456
```

Show changes affecting a specific item:

```sh
fspulse report changes --item-id 789
```

Report formats (`csv`, `table`, `tree`) can be specified using the `--format` option:

```sh
fspulse report items --root-id 123 --format tree
```

## Command-Line Help

For a complete list of commands and options, run:

```sh
fspulse --help
```

## Roadmap

Future improvements and features include:

- Enhanced reporting capabilities
- Expanded file validation types
- Increased resilience to file system errors

## License

FsPulse is released under the MIT License.

