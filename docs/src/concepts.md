# Concepts

fsPulse tracks the state of your filesystem over time using a **temporal versioning** model. The core entities — roots, scans, items, item versions, hash versions, schedules, and tasks — form a layered model for understanding how your data evolves.

---
## Root

A **root** is the starting point for a scan. It represents a specific path on the filesystem that you tell fsPulse to track.

- Paths are stored as absolute paths.
- Each root has a unique ID.
- You can scan a root multiple times over time.

---

## Scan

A **scan** is a snapshot of a root directory at a specific point in time.

Each scan records:
- The time the scan was started and ended
- Whether hashing and validation were enabled
- Counts of files, folders, and total size discovered
- Counts of additions, modifications, and deletions detected
- Counts of files in each validation state and hash state
- Counts of new integrity issues (suspect hashes and validation failures) detected

Scans are always tied to a root via `root_id` and are ordered chronologically by `started_at`.

---

## Item

An **item** represents the stable identity of a single file or folder discovered during scanning. The `items` table stores only identity information:
- Root
- Path
- Name (last path segment)
- Type (File, Directory, Symlink, or Unknown)

An item's mutable state — metadata, hash, validation — is stored in **item versions**, not in the item row itself.

---

## Item Version

An **item version** captures the full known state of an item at a point in time. fsPulse uses temporal versioning: instead of maintaining one mutable row per item, the system stores one row per **distinct state**. A new version is created only when an item's observable state changes.

Each version contains:
- **Temporal range** — `first_scan_id` (when this state was first observed) and `last_scan_id` (last scan where it was confirmed)
- **Deletion status** — whether the item existed or had been deleted
- **Access status** — whether the item could be read successfully
- **Metadata** — modification date and size
- **Validation state** — format validation result and any error message (files only; null for folders)
- **Review timestamps** — `val_reviewed_at` and `hash_reviewed_at` record when a user acknowledged integrity issues on this version (files only)
- **Descendant change counts** — add, modify, delete, and unchanged counts for child items (folders only; null for files)

An item that exists unchanged across 50 scans has exactly **one version row**. You never need to examine multiple versions to reconstruct the current state — each version is a complete snapshot.

### Deriving Change Types

Change types are derived by comparing adjacent versions of an item:
- **Add**: No previous version exists, or the previous version was a deletion
- **Delete**: This version marks the item as deleted
- **Modify**: Previous version exists, neither is deleted, and state differs

---

## Hash Version

A **hash version** tracks the SHA-256 content hash of a file over time, bound to a specific item version. Hash observations are stored in a separate `hash_versions` table.

Each hash version records:
- The hash value
- A **hash state**: Baseline (first hash for a version, or hash change explained by metadata change) or Suspect (hash changed without a metadata change — possible bit rot or tampering)
- A temporal range (`first_scan_id` to `last_scan_id`) indicating when this hash was observed

An item version can have zero hash observations (never hashed), one (stable hash), or multiple (hash changed between scans). See [Scanning - Hash States](scanning.md#hash-states) for details.

---

## Integrity Reviews

Integrity issues — suspect hashes and validation failures — are surfaced on the [Integrity](web_ui/integrity.md) page. Users acknowledge issues by marking them as **reviewed**, which records a timestamp on the item version. Reviews are a lightweight acknowledgment mechanism tracked independently for hash and validation issues on each version.

---

## Schedule

A **schedule** defines automatic recurring scans. Schedules specify:
- Which root to scan
- Timing (daily, weekly, monthly, or interval-based)
- Scan options (hashing mode, validation mode)

Schedules can be enabled or disabled independently.

---

## Task

A **task** is a unit of work in the execution queue. Tasks are created from manual scan requests or triggered by schedules. The Home page shows active and upcoming tasks; the History page shows completed tasks.

Tasks can be paused globally, and individual tasks can be stopped while in progress.

---

## Entity Relationships

```text
Root
 ├── Schedule (recurring scan configuration)
 ├── Scan (one per execution)
 └── Item (stable identity)
      └── Item Version (state at a point in time)
           └── Hash Version (hash observation over time)
```

---

These concepts form the foundation of fsPulse's scanning, browsing, and query capabilities.
