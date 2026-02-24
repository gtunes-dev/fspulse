# Browse

The Browse page is an investigation workbench for navigating your filesystem hierarchy, comparing states across time, and inspecting individual items in detail.

## Browse Cards

Each browse card is a self-contained browsing environment with its own root selection, scan selection, view mode, and optional detail panel.

### Root and Scan Selection

At the top of each card:
- **Root picker**: Select which root directory to browse
- **Scan bar**: Shows the current scan (e.g., "Scan #42 — 15 Jan") with a calendar button to pick a different scan and a "Latest" button to jump to the most recent scan

### View Modes

Three view modes are available via tabs:

#### Tree View

A hierarchical expand/collapse tree showing the filesystem structure at the selected scan point:
- Click the **chevron** on folders to expand or collapse
- Click an **item name** to select it and open the detail panel
- Children are loaded on demand for performance
- Deleted items shown with strikethrough and a trash icon when "Show deleted" is enabled

#### Folder View

A flat, breadcrumb-navigated view similar to a file explorer:
- **Breadcrumb ribbon** at the top shows the current path — click any segment to navigate up
- **Sortable columns**: Name, Size, Modified Date — click column headers to sort (ascending/descending)
- **Folder navigation**: Click a folder's icon to navigate into it
- **Item selection**: Click an item's name to select it for the detail panel
- Directories are always sorted first, then by the selected column

#### Search View

A text search across all items in the selected root and scan:
- Type in the search box to filter items by path (with 300ms debounce)
- Results appear as a flat list with full path tooltips
- Click any result to select it for the detail panel

### Controls

- **Show deleted**: Toggle checkbox to include or hide deleted items across all views
- Each view mode maintains its own independently selected item — switching tabs preserves your selection in each

---

## Detail Panel

Clicking any item opens an inline detail panel within the card. The panel shows comprehensive information about the selected item:

### Current State
- File/folder size (dual format: decimal and binary units)
- Modification time
- Item type
- Current hash (if hashed)
- Validation state and error details

### Size History
- Interactive chart showing how the item's size has changed over time

### Version History
- Paginated timeline of all item versions, showing what changed and when
- Each version displays its temporal range (first scan to last scan)

### Alerts
- Any alerts associated with this item (suspicious hashes, validation failures, access errors)
- Alert status and timestamps

Close the detail panel by clicking the close button. The panel scrolls independently from the navigation area.

---

## Comparison Mode

Click **Show Compare** in the page header to open a second browse card side by side. Each card operates independently — you can:

- **Compare across time**: Same root, different scans — see how the filesystem changed
- **Compare across roots**: Different roots (e.g., original vs. backup) at the same or different scans

When both cards have detail panels open, the panels appear adjacent in the center for easy comparison (Card A's panel is on the right, Card B's panel is on the left).

Close the second card by clicking **Show Compare** again.

---

## Use Cases

- **Investigation**: Drill into specific files when alerts are triggered — click an alert's item, then inspect version history and validation errors
- **Capacity analysis**: Use Folder view sorted by Size to find what's consuming space
- **Change review**: Browse at two scan points in comparison mode to see exactly what changed
- **Verification**: Check hash and validation status of critical files in the detail panel
- **Point-in-time browsing**: Use the scan picker to see the filesystem as it was at any past scan
