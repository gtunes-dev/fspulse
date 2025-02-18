# DirCheck

DirCheck is a command-line tool designed to capture the state of directories and detect changes over time. It records file and directory metadata, tracking additions, deletions, and modifications. DirCheck supports both **shallow scans**, which compare file metadata, and **deep scans**, which compute file hashes to detect content changes even when metadata remains unchanged.

## Overview

DirCheck organizes data into structured entities, each identified by a unique ID that appears in reports and can be used as input for other commands:

- **Root Path**: A directory registered for scanning
- **Scan**: A snapshot of the directory’s state at a given time
- **Entry**: A file or directory recorded in a scan
- **Change**: A modification detected between scans

Scans can be **shallow** (metadata-based) or **deep** (including file content hashing). Deep scans allow detection of changes due to bit rot, corruption, or manual modification when timestamps and sizes remain the same.

By default, DirCheck stores its database in the same directory as the binary. However, you can specify a different location using the `--dbpath` option.

## Installation

To install DirCheck, clone the repository and build it with Cargo:

```sh
cargo build --release
```

Move the compiled binary to a location in your `PATH` or run it from the build directory.

## Usage

### Scanning a Directory

To perform a shallow scan of the current directory:

```sh
dircheck scan
```

To scan a specific directory:

```sh
dircheck scan --path /some/directory
```

To perform a deep scan (including file hashes):

```sh
dircheck scan --deep
```

### Reporting

#### Show the latest scan summary

```sh
dircheck report scans --latest
```

#### Show a specific scan summary (replace `<scan_id>` with an actual scan ID)

```sh
dircheck report scans --id <scan_id>
```

#### Show changes detected in the latest scan

```sh
dircheck report scans --latest --changes
```

#### Show changes detected in a specific scan

```sh
dircheck report scans --id <scan_id> --changes
```

#### Show root paths stored in the database

```sh
dircheck report root-paths
```

#### Show entries recorded in a scan (replace `<entry_id>` with an actual entry ID)

```sh
dircheck report entries --id <entry_id>
```

## Command-Line Help

For a full list of available commands and options, run:

```sh
dircheck --help
```

## Roadmap

Future improvements and features include:

- Completion of all commands and parameters
- Enhanced content and formatting for reports
- Progress indication during deep scans
- Multi-threaded deep scans for parallelized hash computation
- Resumption of incomplete scans
- Improved resilience to file system and access errors

## License

DirCheck is released under the MIT License.
