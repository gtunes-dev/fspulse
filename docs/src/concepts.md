# Concepts

FsPulse tracks the state of your filesystem over time using a **temporal versioning** model. The core entities — roots, scans, items, item versions, alerts, schedules, and tasks — form a layered model for understanding how your data evolves.

---
## Root

A **root** is the starting point for a scan. It represents a specific path on the filesystem that you tell FsPulse to track.

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
- Any alerts generated

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

An **item version** captures the full known state of an item at a point in time. FsPulse uses temporal versioning: instead of maintaining one mutable row per item, the system stores one row per **distinct state**. A new version is created only when an item's observable state changes.

Each version contains:
- **Temporal range** — `first_scan_id` (when this state was first observed) and `last_scan_id` (last scan where it was confirmed)
- **Deletion status** — whether the item existed or had been deleted
- **Access status** — whether the item could be read successfully
- **Metadata** — modification date and size
- **Hash** — SHA-256 content hash (if computed)
- **Validation** — format validation state and any error message

An item that exists unchanged across 50 scans has exactly **one version row**. You never need to examine multiple versions to reconstruct the current state — each version is a complete snapshot.

### Deriving Change Types

Change types are derived by comparing adjacent versions of an item:
- **Add**: No previous version exists, or the previous version was a deletion
- **Delete**: This version marks the item as deleted
- **Modify**: Previous version exists, neither is deleted, and state differs

---

## Alert

An **alert** flags a potential integrity issue detected during scanning. There are three alert types:
- **Suspicious Hash** — file content hash changed but modification time did not, suggesting bit rot or tampering
- **Invalid Item** — format validation detected corruption in a supported file type
- **Access Denied** — FsPulse could not access the item's metadata or contents

Alerts have statuses (Open, Flagged, Dismissed) for triage workflows.

---

## Schedule

A **schedule** defines automatic recurring scans. Schedules specify:
- Which root to scan
- Timing (daily, weekly, monthly, or interval-based)
- Scan options (hashing mode, validation mode)

Schedules can be enabled or disabled independently.

---

## Task

A **task** is a unit of work in the execution queue. Tasks are created from manual scan requests or triggered by schedules. The Tasks page shows active, upcoming, and completed tasks.

Tasks can be paused globally, and individual tasks can be stopped while in progress.

---

## Entity Relationships

```text
Root
 ├── Schedule (recurring scan configuration)
 ├── Scan (one per execution)
 │    └── Alert (integrity issues found)
 └── Item (stable identity)
      └── Item Version (state at a point in time)
```

---

These concepts form the foundation of FsPulse's scanning, browsing, and query capabilities.
