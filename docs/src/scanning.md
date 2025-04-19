# Scanning

FsPulse scans are at the core of how it tracks changes to the file system over time. A scan creates a snapshot of a root directory and analyzes changes compared to previous scans. This page explains how to initiate scans, how incomplete scans are handled, and the phases involved in each scan.

---

## Initiating a Scan

You can start a scan in one of two ways:

- **Command line:**
  ```sh
  fspulse scan --root-path /your/path
  ```
- **Interactive mode:** From the interactive menu, select **Scan** to re-scan a root path that has previously been scanned.

> Interactive mode only supports scanning previously scanned paths. To scan a new root for the first time, use the command line.

Once a scan on a root has begun, it must complete or be explicitly stopped before another scan on the same root can be started. Scans on other roots can run independently.

---

## In-Progress Scans

FsPulse is designed to be resilient to interruptions like system crashes or power loss. If a scan stops before completing, FsPulse saves its state so it can be resumed later.

To resume or discard an in-progress scan:

```sh
fspulse scan --root-path /your/path
```

If a scan is in progress, FsPulse will prompt you to:

- **Resume** the scan from where it left off
- **Stop** the scan and discard its partial results

> Stopping a scan reverts the database to its pre-scan state. All detected changes, computed hashes, and validations from that partial scan will be discarded.

---

## Phases of a Scan

Each scan proceeds in three main phases:

### 1. Discovery

The directory tree is deeply traversed. For each file or folder encountered:

- If not seen before:
  - A new item is created
  - An **Add** change is recorded
- If seen before:
  - FsPulse compares current file system metadata:
    - **Modification date** (files and folders)
    - **File size** (files only)
  - If metadata differs, the item is updated and a **Modify** change is recorded
- If the path matches a **tombstoned** item (previously deleted):
  - If type matches (file/folder), the tombstone is reactivated and an **Add** change is created
  - If type differs, FsPulse creates a new item and new **Add** change

> Files and folders are treated as distinct types. A single path that appears as both a file and folder at different times results in two separate items.

---

### 2. Sweep

FsPulse identifies items not seen during the current scan:

- Any item that:
  - Is **not** a tombstone, and
  - Was **not visited** in the scan

...is marked as a **tombstone**, and a **Delete** change is created.

Moved files appear as deletes and adds, as FsPulse does not yet track move operations.

---

### 3. Analysis

This phase runs only if the scan is started with `--hash` and/or `--validate`.

- **Hashing** — Computes an MD5 hash of file contents
- **Validation** — Uses file-type-specific validators to check content integrity

If either the hash or validation result changes:

- If an **Add** or **Modify** change already exists, the new data is attached to it
- Otherwise, a new **Modify** change is created

Each change stores both the **old** and **new** values for comparison.

---

## Performance and Threading

The analysis phase runs in parallel:

- Default: **8 threads**
- User-configurable in [Configuration](configuration.md)

---

## Summary of Phases

| Phase     | Purpose                                                          |
| --------- | ---------------------------------------------------------------- |
| Discovery | Finds and records new or modified items                          |
| Sweep     | Marks missing items as tombstones and records deletions          |
| Analysis  | Computes hashes/validations and records changes if values differ |

Each scan provides a consistent view of the file system at a moment in time and captures important differences across revisions.

