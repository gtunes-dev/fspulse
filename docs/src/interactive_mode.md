# Interactive Mode

FsPulse includes an interactive mode that provides a menu-driven interface for common tasks.

![Interactive menu](https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/interact.png)

To launch interactive mode:

```sh
fspulse interact
```

---

## Overview

The interactive menu offers the following options:

- **Scan** — Re-scan a previously scanned root
- **Query** — Run custom queries using the query language
- **Report** — View predefined summary reports
- **Exit** — Close interactive mode

---

## Scan

This option lets you scan a folder that has already been scanned.

> ⚠️ You must first perform a scan from the command line using:

```sh
fspulse scan --root-path /your/path
```

Once a root has been scanned at least once, it becomes available in the interactive menu.

Interactive scans allow you to toggle:
- **Hashing** — compute MD5 file hashes
- **Validation** — check file content integrity for supported types

---

## Query

Allows you to enter queries using FsPulse’s [query syntax](query.md).

- Use full expressions like:
  ```sh
  items where item_path:('photos')
  changes where val_new:(I) show default, val_old, val_new
  ```
- Queries may be repeated until you type `q` or `exit`
- Query errors provide detailed syntax feedback
- Use the ↑ and ↓ arrow keys to scroll through previous entries in your session

---

## Report

Provides quick access to common predefined reports:

- List all roots
- Show recent scans
- Display invalid items
- View changes from the latest scan

Reports are internally implemented as saved queries and will expand over time.

---

Interactive mode is especially helpful for exploring your data once scans are available, and for learning the query syntax interactively.

