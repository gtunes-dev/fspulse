# Browse

The Browse page provides an intuitive interface for navigating your filesystem hierarchy and inspecting individual items in detail.

## Filesystem Tree

Navigate your scanned directories with a hierarchical tree view showing:
- Folders and files from your most recent scan
- Item counts and sizes
- Visual indicators for items with alerts

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-browse-tree.png" alt="Browse Filesystem Tree" style="width: 90%; max-width: 900px;">
</p>

### Features

- **Search**: Filter items by path using the search box
- **Expand/Collapse**: Navigate the folder structure
- **Sort**: Order by name, size, or modification time
- **Root Selection**: Switch between different scan roots

## Item Detail Panel

Click any item to open the detail panel, which provides comprehensive information:

<p align="center">
  <img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/web-browse-detail.png" alt="Item Detail Panel" style="width: 90%; max-width: 900px;">
</p>

### Metadata

- File/folder size (dual format: decimal and binary units)
- Modification time
- Item type
- Current hash (if hashed)

### Validation Status

- Validation state (Valid, Invalid, NotValidated)
- Validation error details (if any)
- Last validation scan information

### Change History

- Change type across scans (Add, Modify, Delete, NoChange)
- Hash changes detected
- Modification timeline

### Associated Alerts

- Suspicious hash changes
- Validation failures
- Alert status and timestamps

## Use Cases

- **Investigation**: Drill down into specific files or folders when alerts are triggered
- **Verification**: Check hash and validation status of critical files
- **Analysis**: Understand what changed between scans
- **Navigation**: Visual exploration of your monitored filesystems
