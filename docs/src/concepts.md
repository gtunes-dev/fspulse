# Concepts

FsPulse is centered around tracking and understanding the state of the file system over time. The core entities in FsPulse — roots, scans, items, and changes — represent a layered model of this information.

---

## Root

A **root** is the starting point for a scan. It represents a specific path on the file system that you explicitly tell FsPulse to track.

Each root is stored persistently in the database, and every scan you perform refers back to a root.

- Paths are stored as absolute paths.
- Each root has a unique ID.
- You can scan a root multiple times over time.

---

## Scan

A **scan** is a snapshot of a root directory at a specific point in time.

Each scan records metadata about:
- The time the scan was performed
- Whether hashing and validation were enabled
- The collection of items (files and folders) found during the scan

Scans are always tied to a root via `root_id`, and are ordered chronologically by `scan_time`.

---

## Item

An **item** represents a single file or folder discovered during a scan.

Each item includes metadata such as:
- Path
- Whether it's a file or directory
- Last modified date
- Size
- Optional hash and validation info

Items are created when newly seen, and marked with a tombstone (`is_ts = true`) if they were present in previous scans but no longer exist.

---

## Change

A **change** represents a detected difference in an item between the current scan and a previous one.

Changes may reflect:
- File additions
- File deletions
- Metadata or content modifications

Each change is associated with both the scan and the item it affects.

---

## Entity Flow

A simplified representation of how the entities relate:

```
Root
 └── Scan (per run)
      └── Item (files and folders)
           └── Change (if the item changed)
```

---

These concepts form the foundation of FsPulse’s scan and query capabilities. Understanding them will help you make the most of both interactive and command-line modes.