# Browse

The Browse page is an investigation workbench for navigating your filesystem hierarchy, comparing states across time, and inspecting individual items in detail.

<!-- Screenshot: Browse page showing tree view with detail panel open -->
<!-- ![Browse Page Overview](screenshot-placeholder-browse-overview.png) -->

## Browse Cards

Each browse card is a self-contained browsing environment with its own root selection, scan selection, view mode, and optional detail panel.

### Root and Scan Selection

At the top of each card:
- **Root picker**: Select which root directory to browse. If you navigated here from another page with a root selected, it will be pre-selected via the shared root context.
- **Scan bar**: Shows the current scan (e.g., "Scan #42 — 15 Jan at 10:30") with a calendar button to pick a different scan and a "Latest" button to jump to the most recent scan.

The calendar picker highlights dates that have scans and disables dates without scans. When you select a date, it shows all scans for that day with their change summaries (adds, modifications, deletions). If you select a date with no scans, the nearest available scan is shown.

### View Modes

Three view modes are available via tabs:

#### Tree View

A hierarchical expand/collapse tree showing the filesystem structure at the selected scan point:
- Click the **chevron** on folders to expand or collapse
- Click an **item name** to select it and open the detail panel
- Children are loaded on demand for performance
- Folder expansion state is preserved when switching between scans or when the tree refreshes
- Deleted items shown with strikethrough and a trash icon when "Show deleted" is enabled
- Items are color-coded by change type: green (added), blue (modified), red (deleted), gray (unchanged)

#### Folder View

A flat, breadcrumb-navigated view similar to a file explorer:
- **Breadcrumb ribbon** at the top shows the current path — click any segment to navigate up
- **Sortable columns**: Name, Size, Modified Date — click column headers to sort (ascending/descending)
- **Folder navigation**: Click a folder's icon to navigate into it
- **Item selection**: Click an item's name to select it for the detail panel
- Directories are always sorted first, then by the selected column
- When switching from Folder view to Tree view, the tree automatically reveals and expands to your current folder location

#### Search View

A text search across all items in the selected root and scan:
- Type in the search box to filter items by path (with debounce)
- Results appear as a flat list with parent path context shown below each item name
- Click any result to select it for the detail panel
- Typing in the search box automatically switches to the Search tab

### Integrity Filters

A collapsible filter panel lets you narrow the view by integrity status across three dimensions:

**Change Kind**: Filter by Added, Modified, Deleted, or Unchanged items.

**Hash State**: Filter by Baseline, Unknown, or Suspect hash states. Folders are shown if they contain descendants matching the selected states.

**Validation State**: Filter by Valid, Invalid, Unknown, or No Validator states. Works the same as hash state filtering with descendant logic for folders.

Items must pass all active filter dimensions to be visible. Active filters are indicated with visual highlights on the filter buttons.

<!-- Screenshot: Browse page with integrity filters active, showing filtered tree view -->
<!-- ![Browse Filters](screenshot-placeholder-browse-filters.png) -->

### Controls

- **Show deleted**: Toggle to include or hide deleted items across all views
- Each view mode maintains its own independently selected item — switching tabs preserves your selection in each

---

## Detail Panel

Clicking any item opens an inline detail panel within the card. The panel can be positioned on either side of the card using the flip button.

<!-- Screenshot: Detail panel showing file information, size history chart, and version history -->
<!-- ![Detail Panel](screenshot-placeholder-browse-detail.png) -->

### Item Information
- File/folder type label and icon
- File size (formatted compactly)
- Modification time
- Path with deletion status indicator
- First seen scan and total version count

### File Integrity (Files Only)
- **Hash state**: Displayed as Baseline, Suspect, or Unknown with a colored icon. Click to expand/collapse the full SHA-256 hash value.
- **Validation state**: Displayed as Valid, Invalid, Unknown, or No Validator. If invalid, the validation error message is shown.

### Directory Children Counts (Directories Only)
- Immediate file and folder counts
- Change breakdown: added (green), modified (blue), deleted (red), unchanged (gray)
- Integrity breakdown: suspect hash count, invalid item count

### Size History
- Interactive line chart showing how the item's size has changed over time
- Configurable time window: 7 days, 30 days, 3 months, 6 months, 1 year

### Version History
- Chronological list of all item versions, each showing its change type (Initial, Modified, Deleted, Restored)
- Each version displays its temporal range (first scan to last scan)
- Expand a version to see exactly what changed: modification date, size, hash, access state, validation state, hash state
- For directories: shows changes in descendant counts and integrity counts
- The version corresponding to the currently viewed scan is highlighted with an eye icon
- Load older versions on demand

### Alerts
- Any alerts associated with this item (suspect hashes, validation failures, access errors)
- Each alert shows its type, timestamp, and details
- **Alert status is editable directly from the panel** — use the dropdown to change between Open, Flagged, and Dismissed without leaving the Browse page
- Load more alerts on demand

Close the detail panel by clicking the close button.

---

## Comparison Mode

Click **Show Compare** in the page header to open a second browse card side by side. Each card operates independently — you can:

- **Compare across time**: Same root, different scans — see how the filesystem changed
- **Compare across roots**: Different roots (e.g., original vs. backup) at the same or different scans

When both cards have detail panels open, the panels appear adjacent in the center for easy comparison (Card A's panel is on the right, Card B's panel is on the left).

Close the second card by clicking **Show Compare** again.

<!-- Screenshot: Comparison mode with two browse cards side by side -->
<!-- ![Comparison Mode](screenshot-placeholder-browse-compare.png) -->

---

## Use Cases

- **Investigation**: Drill into specific files when alerts are triggered — click an alert's item, then inspect version history and validation errors
- **Capacity analysis**: Use Folder view sorted by Size to find what's consuming space
- **Change review**: Browse at two scan points in comparison mode to see exactly what changed
- **Integrity audit**: Use hash state and validation state filters to focus on files with suspect or invalid states
- **Verification**: Check hash and validation status of critical files in the detail panel
- **Point-in-time browsing**: Use the scan picker to see the filesystem as it was at any past scan
