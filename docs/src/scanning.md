# Scanning

FsPulse scans are at the core of how it tracks changes to the file system over time. A scan creates a snapshot of a root directory and analyzes changes compared to previous scans. This page explains how to initiate scans, how incomplete scans are handled, and the phases involved in each scan.

---

## Initiating a Scan

**FsPulse runs as a web service, and scans are initiated through the web UI:**

1. Start the server: `fspulse serve`
2. Open http://localhost:8080 in your browser (or the custom port you've configured)
3. Navigate to the **Monitor** page
4. Configure scan options (hashing, validation)
5. Start the scan and monitor real-time progress on the **Home** page

The web UI supports both scheduled automatic scans and manual on-demand scans. You can create recurring schedules (daily, weekly, monthly, or custom intervals) or initiate individual scans as needed. See [Monitor](web_ui/monitor.md) for details.

Once a scan on a root has begun, it must complete or be explicitly stopped before another scan on the same root can be started. Scans on different roots can run independently.

> **Note**: FsPulse is designed with the web UI as the primary interface for all users. A [command-line interface](cli.md) is also available for expert users who prefer terminal workflows or need to integrate FsPulse into scripts and automation, but all functionality is accessible through the web UI.

---

## Hashing

Hashing is a key capabilities of FsPulse.

FsPulse uses the standard SHA2 (256) message-digest algorithm to compute digital fingerprints of file contents.
The intent of hashing is to enable the detection of changes to file content in cases where the modification
date and file size have not changed. One example of a case where this might occur is bit rot (data decay).

When configuring a scan in the web UI, you can enable hashing with these options:
- **Hash changed items** (default): Compute hashes for items that have never been hashed or whose file size or modification date has changed
- **Hash all items**: Hash all files, including those that have been previously hashed

If a hash is detected to have changed, a change record is created and an alert is generated (see [Alerts](web_ui/alerts.md)).

### Finding Hash Changes

You can investigate hash changes through the web UI:
- **[Alerts Page](web_ui/alerts.md)**: Shows suspicious hash changes where file metadata hasn't changed
- **[Explore Page](web_ui/explore.md)**: Use the **Query** tab to run custom queries

Example query to find hash changes without metadata changes (run on the Explore page's Query tab):
```text
changes where meta_change:(F), hash_change:(T) show default, item_path order by change_id desc
```

## Validating

FsPulse can attempt to assess the "validity" of files. 

FsPulse uses community-contributed libraries to "validate" files. Validation is implemented as
opening and reading or traversing the file. These community libraries raise a variety of "errors"
when invalid content is encountered.

FsPulse's ability to validate files is limited to the capabilities of the libraries that it uses,
and these libraries vary in terms of completeness and accuracy. In some cases, such as FsPulse's use
of lopdf to validate PDF files, false positive "errors" may be detected as a consequence of lopdf
encountering PDF file contents it does not yet understand. Despite these limitations, FsPulse
offers a unique and effective view into potential validity issues in files.

See [Validators](validators.md) for the complete list of supported file types.

When configuring a scan in the web UI, you can enable validation with these options:
- **Validate changed items** (default): Validate files that have never been validated or have changed in terms of modification date or size
- **Validate all items**: Validate all files regardless of previous validation status

### Validation States

Validation states are stored in the database as:
- **U**: Unknown. No validation has been performed
- **N**: No Validator. No validator exists for this file type
- **V**: Valid. Validation was performed and no errors were encountered
- **I**: Invalid. Validation was performed and an error was encountered

In the case of 'I' (Invalid), the validation error message is stored alongside the validation state. When an item's validation state changes, the change is recorded and both old and new states are available for analysis.

If a validation pass produces an error identical to a previously seen error, no new change is recorded.

### Finding Validation Issues

Invalid items are automatically flagged as alerts. You can investigate validation failures through the web UI:
- **[Alerts Page](web_ui/alerts.md)**: Shows all items with validation failures, with filtering and status management
- **[Browse Page](web_ui/browse.md)**: Click any item to see its validation status and error details
- **[Explore Page](web_ui/explore.md)**: Use the **Query** tab to run custom queries

Example query to find validation state changes (run on the Explore page's Query tab):
```text
changes where val_change:(T) show default, item_path order by change_id desc
```

Additional queries can filter on specific old and new validation states. See [Query Syntax](query.md) for details.

---

## In-Progress Scans

FsPulse is designed to be resilient to interruptions like system crashes or power loss. If a scan stops before completing, FsPulse saves its state so it can be resumed later.

When you attempt to start a new scan on a root that has an in-progress scan, the web UI will prompt you to:

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

This phase runs only if hashing and/or validation is enabled when configuring the scan (see [Hashing](#hashing) and [Validating](#validating) above).

- **Hashing** — Computes a SHA2 hash of file contents
- **Validation** — Uses file-type-specific validators to check content integrity (see [Validators](validators.md))

If either the hash or validation result changes:

- If an **Add** or **Modify** change already exists, the new data is attached to it
- Otherwise, a new **Modify** change is created

Each change record stores both the **old** and **new** values for comparison, allowing you to track exactly what changed.

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

