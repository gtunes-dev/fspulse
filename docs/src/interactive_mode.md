# Interactive Mode

FsPulse provides two terminal-based interactive interfaces for working with your data:

- **`fspulse interact`** — Menu-driven interface (documented on this page)
- **`fspulse explore`** — Visual data explorer TUI (see below)

Both provide alternative ways to interact with FsPulse without needing the web UI.

---

## Interactive Menu (`fspulse interact`)

The `interact` command provides a menu-driven interface for common tasks.

![Interactive menu](https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/interact.png)

To launch:

```sh
fspulse interact
```

### Available Options

The interactive menu offers:

- **Query** — Run custom queries using the query language
- **Explore** — Launch the visual data explorer
- **Report** — View predefined summary reports
- **Exit** — Close interactive mode

> **Note:** Scanning is performed exclusively through the web UI. Use `fspulse serve` to access the web interface where you can create and manage scans.

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

## Data Explorer (`fspulse explore`)

The `explore` command provides a visual, terminal-based data explorer—a full-screen TUI for browsing your FsPulse data.

To launch:

```sh
fspulse explore
```

### Key Features

- **Visual Interface**: Spreadsheet-like display of roots, scans, items, and changes
- **Keyboard Navigation**: Use arrow keys, Tab, and shortcuts to navigate
- **Entity Views**: Switch between different data types (roots, scans, items, changes)
- **Filtering & Sorting**: Interactive controls for refining results
- **Full-Screen**: Maximizes terminal space for data exploration

### Differences from `interact`

| Feature | `interact` | `explore` |
|---------|------------|-----------|
| **Interface** | Menu-driven prompts | Full-screen visual TUI |
| **Navigation** | Text-based selection | Arrow keys, visual navigation |
| **Data Display** | List-based output | Table/grid layout |
| **Best For** | Quick queries and reports | Visual data exploration |

### Docker Usage

Both interactive modes work in Docker via `docker exec` with the `-it` flags:

```sh
# Interactive menu
docker exec -it fspulse fspulse interact

# Data explorer
docker exec -it fspulse fspulse explore
```

**Important**: The `-it` flags are required for proper terminal interaction.

---

## Summary

FsPulse offers multiple ways to interact with your data:

- **Web UI** — Full-featured browser interface (via `fspulse serve`)
- **Interactive Menu** — Quick access to common tasks (via `fspulse interact`)
- **Data Explorer** — Visual terminal-based exploration (via `fspulse explore`)
- **Direct CLI** — Scriptable commands for automation (see [Command-Line Interface](cli.md))

Choose the interface that best fits your workflow.

