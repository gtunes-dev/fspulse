# Scanning

FsPulse scans are at the core of how it tracks changes to the filesystem over time. A scan creates a snapshot of a root directory and detects changes compared to previous scans. This page explains how to initiate scans, how incomplete scans are handled, and the phases involved in each scan.

---

## Initiating a Scan

**FsPulse runs as a web service, and scans are initiated through the web UI:**

1. Start the server: `fspulse serve`
2. Open http://localhost:8080 in your browser (or the custom port you've configured)
3. Navigate to the **Monitor** page to configure roots and schedules
4. Start a manual scan or let a schedule trigger one
5. Monitor real-time progress on the **Tasks** page

The web UI supports both scheduled automatic scans and manual on-demand scans. You can create recurring schedules (daily, weekly, monthly, or custom intervals) or initiate individual scans as needed. See [Monitor](web_ui/monitor.md) for details.

Once a scan on a root has begun, it must complete or be explicitly stopped before another scan on the same root can be started. Scans on different roots can run independently.

---

## Hashing

Hashing is a key capability of FsPulse.

FsPulse uses the standard SHA2 (256) message-digest algorithm to compute digital fingerprints of file contents. The intent of hashing is to enable the detection of changes to file content in cases where the modification date and file size have not changed. One example of a case where this might occur is bit rot (data decay).

When configuring a scan in the web UI, you can enable hashing with these options:
- **Hash changed items** (default): Compute hashes for items that have never been hashed or whose file size or modification date has changed
- **Hash all items**: Hash all files, including those that have been previously hashed

If a hash is detected to have changed, a new item version is created and an alert is generated (see [Alerts](web_ui/alerts.md)).

### Finding Hash Changes

You can investigate hash changes through the web UI:
- **[Alerts Page](web_ui/alerts.md)**: Shows suspicious hash changes where file metadata hasn't changed
- **[Browse Page](web_ui/browse.md)**: Click any item to see its version history including hash changes
- **[Explore Page](web_ui/explore.md)**: Use the **Query** tab to run custom queries

Example query to find items with suspicious hashes (run on the Explore page's Query tab):
```text
alerts where alert_type:(H) order by created_at desc
```

## Validating

FsPulse can attempt to assess the "validity" of files.

FsPulse uses community-contributed libraries to "validate" files. Validation is implemented as opening and reading or traversing the file. These community libraries raise a variety of "errors" when invalid content is encountered.

FsPulse's ability to validate files is limited to the capabilities of the libraries that it uses, and these libraries vary in terms of completeness and accuracy. In some cases, such as FsPulse's use of lopdf to validate PDF files, false positive "errors" may be detected as a consequence of lopdf encountering PDF file contents it does not yet understand. Despite these limitations, FsPulse offers a unique and effective view into potential validity issues in files.

See [Validators](validators.md) for the complete list of supported file types.

When configuring a scan in the web UI, you can enable validation with these options:
- **Validate changed items** (default): Validate files that have never been validated or have changed in terms of modification date or size
- **Validate all items**: Validate all files regardless of previous validation status

### Validation States

Validation applies only to files — folders do not have a validation state. Validation states are stored in the database as:
- **U**: Unknown. No validation has been performed
- **N**: No Validator. No validator exists for this file type
- **V**: Valid. Validation was performed and no errors were encountered
- **I**: Invalid. Validation was performed and an error was encountered

In the case of 'I' (Invalid), the validation error message is stored alongside the validation state. When an item's validation state changes, a new item version is created capturing both the old and new states.

If a validation pass produces an error identical to the existing error, no new version is created — only the `last_val_scan` bookkeeping field is updated.

### Finding Validation Issues

Invalid items are automatically flagged as alerts. You can investigate validation failures through the web UI:
- **[Alerts Page](web_ui/alerts.md)**: Shows all items with validation failures, with filtering and status management
- **[Browse Page](web_ui/browse.md)**: Click any item to see its validation status and error details in the inline detail panel
- **[Explore Page](web_ui/explore.md)**: Use the **Query** tab to run custom queries

Example query to find items currently in an invalid validation state:
```text
items where val:(I) show default, val_error order by item_path
```

Additional queries can filter on specific validation states. See [Query Syntax](query.md) for details.

---

## In-Progress Scans

FsPulse is designed to be resilient to interruptions like system crashes or power loss. If a scan stops before completing, FsPulse saves its state so it can be resumed later.

When you attempt to start a new scan on a root that has an in-progress scan, the web UI will prompt you to:

- **Resume** the scan from where it left off
- **Stop** the scan and discard its partial results

> Stopping a scan reverts the database to its pre-scan state using an undo log. All detected versions, computed hashes, and validations from that partial scan will be discarded.

---

## Phases of a Scan

Each scan proceeds in three main phases:

### 1. Scanning

The directory tree is deeply traversed. For each file or folder encountered:

- If not seen before:
  - A new item identity is created
  - A new item version is inserted with `first_scan_id = current_scan`
- If seen before:
  - FsPulse compares current filesystem metadata:
    - **Modification date** (files and folders)
    - **File size** (files only)
  - If metadata differs, a new item version is created carrying forward unchanged properties (hash, validation) from the previous version
  - If unchanged, the existing version's `last_scan_id` is updated in place
- If the path matches a **deleted** item (previous version has `is_deleted = true`):
  - A new version is created with `is_deleted = false` (rehydration)

> Files and folders are treated as distinct types. A single path that appears as both a file and folder at different times results in two separate items.

Writes are batched (100 items per transaction) for performance. An undo log records in-place updates to support rollback if the scan is stopped.

---

### 2. Sweeping

FsPulse identifies items not seen during the current scan:

- Any item whose current version is not deleted and was not visited in this scan gets a new version with `is_deleted = true`.

Moved files appear as deletes and adds, as FsPulse does not track move operations.

---

### 3. Analyzing

This phase runs only if hashing and/or validation is enabled (see [Hashing](#hashing) and [Validating](#validating) above).

- **Hashing** — Computes a SHA-256 hash of file contents
- **Validation** — Uses file-type-specific validators to check content integrity (see [Validators](validators.md))

If a hash or validation result changes:

- If the item already received a new version in the scanning phase (same scan), the existing version is **updated in place**
- Otherwise, the previous version's `last_scan_id` is restored and a new version is created

This guarantees **at most one new version per item per scan**.

If the hash and validation results are unchanged, only the bookkeeping fields (`last_hash_scan`, `last_val_scan`) are updated on the existing version.

---

## Performance and Threading

The analysis phase runs in parallel:

- Default: **8 threads**
- Configurable from 1 to 24 in [Configuration](configuration.md)

---

## Summary of Phases

| Phase     | Purpose                                                             |
| --------- | ------------------------------------------------------------------- |
| Scanning  | Traverses the filesystem, creates or updates item versions          |
| Sweeping  | Marks missing items as deleted with new version rows                |
| Analyzing | Computes hashes and validates files, updating or creating versions  |

Each scan provides a consistent view of the filesystem at a moment in time. The temporal versioning model means you can reconstruct the exact state of any item at any scan point.
