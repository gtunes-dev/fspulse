# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [v0.3.1] - 2025-11-23

### Changed
- **Database connection handling**: Refactored to use R2D2 connection pool for improved concurrency and resource management
- **Batch updates during scan phase of scans**: Now transacting in batches rather than for each item
- **Tracing during scans**: When the FsPulse log level is set to tracing, we now trace timing events for the scan phase of scans
- **Log events have time signatures**: All log events now include time signatures

### Fixed
- **Settings page**: Active configuration values now readable in light mode with proper background colors

## [v0.3.0] - 2025-11-18

### Added
- **Query tab pagination**: Pagination support for Explore > Query tab to prevent browser crashes from large result sets
  - Results paginated with 25 rows per page
  - User-specified LIMIT/OFFSET clauses respected when paginating
  - Empty result sets display "No results found" message

### Changed
- **Explore page**: Tab state preserved when switching between tabs (filters, sort, column visibility/order, pagination)
  - Reset button restores columns to default settings
- **Number formatting**: Large numbers in pagination displays now show thousand separators (e.g., "1,205,980")

### Fixed
- **Item detail sheet**: Calendar widget no longer appears over modification date entries in History card

### Highlights from v0.2.x

This release includes all features from the v0.2 series:

- **Browse page**: Virtualized tree view supporting 100k-1M+ items with lazy loading and efficient search
- **Global pause**: Pause all scanning with flexible durations; scans resume automatically
- **Configuration UI**: Full settings management through web UI with validation and visual indicators
- **Scan scheduling**: Daily, weekly, monthly, and interval-based recurring scans
- **Scan history**: Paginated history table with duration, schedule source, and root filtering
- **Insights**: Interactive charts showing file size, counts, changes, and alerts over time
- **Item detail view**: Sliding panel with metadata, validation status, change history, and alerts
- **Folder sizes**: Directory sizes computed during scan with dual-format display
- **Database compaction**: Reclaim space from deleted data via Settings page
- **Graceful shutdown**: Server waits for active scans to complete before exiting
- **Platform data directories**: Database stored in standard OS locations (Linux/macOS/Windows)

## [v0.2.12] - 2025-11-17

### Changed
- **Browse page rewrite**: Completely redesigned to support extremely large file trees (100k-1M+ items)
  - Tree view now uses virtualization and lazy loading for fast performance at any scale
  - Directories load children on-demand when expanded
  - Search displays results as a flat, paginated list with path tooltips instead of a tree
  - "Show deleted" toggle works instantly without reloading data
- **Manual Scan dialog**: Auto-selects root directory when only one root is configured

## [v0.2.11] - 2025-11-17

### Added
- **Global Pause Feature**: Temporarily pause all scanning activity with flexible duration options
  - Pause for 5 minutes, 15 minutes, 1 hour, 24 hours, until tomorrow (12am), or indefinitely
  - Edit pause duration or unpause early through the unified pause management dialog
  - In-progress scans are gracefully stopped and resume automatically when unpaused
  - Pause state persists across application restarts
  - Visual indicators throughout the UI show pause status and resume timing
  - Paused scans appear in Upcoming Scans table with "Paused" status until they resume
  - Real-time WebSocket updates ensure all UI components reflect current pause state

### Changed
- **Scans page** (formerly "Activity"): Renamed to better reflect its purpose as the main dashboard for scan status and history
  - Unified scan control interface with improved visual hierarchy and design consistency
  - Manual Scan and Pause controls always visible in a single action bar
  - Pause button turns purple when system is paused for better visibility
  - Global pause banner appears prominently at top of page when scanning is paused
  - Pause banner now shows friendly duration (e.g., "for 3 hours") in addition to the end time
  - Streamlined active scan display with reduced redundancy
- **Monitor page improvements**: Better visual feedback and real-time updates
  - In-progress scans now show green "In Progress" badge instead of blue for better visibility
  - Incomplete scans show purple "Paused" badge when system is globally paused, making pause state more obvious
  - Roots table now updates immediately when scans complete, scans are scheduled, or roots are added
- **Build optimizations**: Changed from global `codegen-units=1` to per-package optimization for performance-critical dependencies (claxon, sha2, md-5, image, png, lopdf). This significantly reduces compilation time while maintaining runtime performance for file validation and hashing operations.
- **UI color consistency**: Queued scans now use purple icons instead of orange, reserving orange for warnings

### Fixed
- **Upcoming Scans display**: First queued scan now correctly shows "When unpaused" when global pause is active
- **Monitor page Roots table**: Now refreshes automatically when scans complete, scans are scheduled, or new roots are added

### Highlights from v0.2.10
This release includes all features from v0.2.10:
- **Configuration UI**: Full configuration management through Settings page with validation and visual indicators
- **Default data directory changed**: Database now stored in platform-specific data directory (see v0.2.10 for migration options)
- **Environment variable renamed**: `FSPULSE_DATABASE_PATH` → `FSPULSE_DATABASE_DIR`

## [v0.2.10] - 2025-11-15

### Added
- **Configuration UI**: New Settings page provides full configuration management through the web UI
  - View all configuration settings with their current values, sources (environment, config file, or default), and precedence
  - Edit settings directly in the UI with validation
  - See which settings require restart and track pending changes
  - Delete settings from config file to revert to defaults
  - Visual indicators show which value is currently active

### Changed
- Deprecated configuration keys now emit warnings at startup instead of causing errors: `FSPULSE_ANALYSIS_HASH` (environment variable) and `analysis.hash` (config.toml)

### Breaking Changes
- **Environment variable renamed**: `FSPULSE_DATABASE_PATH` → `FSPULSE_DATABASE_DIR` (reflects that it's a directory, not a file path)
- **Configuration field renamed**: `database.path` → `database.dir` in config.toml
- **Default data directory location changed** (native installations only, Docker unaffected):
  - **Old location**: Home directory (`/home/alice/fspulse.db`)
  - **New location**: Platform-specific data directory:
    - Linux: `~/.local/share/fspulse/fspulse.db`
    - macOS: `~/Library/Application Support/fspulse/fspulse.db`
    - Windows: `%LOCALAPPDATA%\fspulse\data\fspulse.db`

  **Migration options** (choose one):

  1. **Move database to new location** (recommended):
     ```bash
     # Linux/macOS
     mkdir -p ~/.local/share/fspulse
     mv ~/fspulse.db ~/.local/share/fspulse/
     mv ~/config.toml ~/.local/share/fspulse/
     ```

  2. **Set database directory to old location** via environment variable:
     ```bash
     export FSPULSE_DATABASE_DIR=$HOME
     fspulse serve
     ```

  3. **Set database directory in config file**:
     ```toml
     [database]
     dir = "/home/alice"  # Use your home directory path
     ```

## [v0.2.9] - 2025-11-11

### Critical Fix
- **Schedule Deletion**: Schedules can now be deleted from the UI without database errors. Deletion uses soft delete (tombstoning) to maintain referential integrity with scans that reference deleted schedules. Historical scan data preserves schedule names even after schedule deletion.

### Highlights from v0.2.8
This release includes all features from v0.2.8:
- **Scan History Table**: Full pagination (25 per page) with Schedule and Duration columns showing scan source and execution time
- **Root Filtering**: Filter scan history by specific root or view all roots
- **Database Schema v11**: Scan timing fields (`started_at`, `ended_at`, `was_restarted`, `schedule_id`)
- **Directory Contents Visualization**: ItemDetailSheet shows file and folder counts
- **Breaking Change**: `scan_time` renamed to `started_at` in FsPulse Query Language

## [v0.2.8] - 2025-11-12

**Note**: v0.2.8 had a critical bug preventing schedule deletion. Please use v0.2.9 or later.

### Added
- Scan History table with full pagination (25 per page), replacing limited Recent Scans view
- Schedule and Duration columns showing scan source and execution time with visual indicators
- Root filter dropdown for viewing scan history by specific root or all roots
- Directory contents visualization in ItemDetailSheet with file and folder counts
- Database schema v11: scan timing fields (`started_at`, `ended_at`, `was_restarted`, `schedule_id`)
- Scan restart detection for tracking scans resumed after application restart

### Changed
- Scan History displays only terminal states (Completed, Error, Stopped)
- Frontend component structure reorganized for improved maintainability

### Fixed
- CI migrated from deprecated macOS 13 to macOS 15 Intel and latest ARM builds

### Breaking Changes
- **FsPulse Query Language**: `scan_time` renamed to `started_at` in scans table. Update existing queries accordingly.

## [v0.2.7] - 2025-11-11

### Improved
- Graceful shutdown handling: server now waits for active scans to complete before exiting, preventing data corruption and allowing scan resumption on restart

## [v0.2.6] - 2025-11-10

### Improved
- Explore page redesigned with card-based layout and styled tab navigation for better visual hierarchy
- Alerts page updated to use consistent RootCard component with improved filter controls

### Fixed
- Docker container permissions issue impacting Synology users
- MD5 hash function removed from config and documentation (it was only partially supported)

## [v0.2.5] - 2025-11-09

### Added
- Database compaction feature in Settings page to reclaim wasted space from deleted data and migrations
- Privacy guarantees prominently displayed in README and documentation (read-only and local-only)
- Privacy guarantees shown on Activity page first-run experience for new users

### Fixed
- CI workflow now sets environment variables for git metadata to ensure correct branch name in version info instead of "HEAD"

## [v0.2.4] - 2025-11-09

### Added
- Settings page with application version, build date, git commit, and git branch information
- Links to GitHub, Documentation, crates.io, and Docker Hub on Settings page
- API endpoint `/api/app-info` to expose build and version metadata
- Build-time capture of git metadata with proper fallbacks for local, CI, and Docker builds

### Fixed
- Header progress bar click now correctly navigates to Activity page instead of broken `/scan` route

## [v0.2.3] - 2025-11-09

### Added
- Build script (`scripts/build.sh`) automates frontend and backend build process
- Build-time validation in `build.rs` ensures frontend assets are built before Rust compilation
- Comprehensive "Building from Source" documentation with troubleshooting guide

### Fixed
- Activity page first-run UX: Manual Scan button now visible when roots are configured but no scans exist
- Activity page now shows normal operational state when scans exist even if all roots have been deleted
- Activity page empty state messaging improved with more actionable guidance for new users

## [v0.2.2] - 2025-11-09

### Fixed
- Browse view now shows a message when a root is being scanned instead of displaying "No items found"
- Monitor page crashing when displaying scans with null file/folder counts

## [v0.2.1] - 2025-11-08

### Breaking Changes

**⚠️ CLI Scan Removal**
- The `scan` subcommand has been removed. All scanning operations must now be performed through the web UI (`fspulse serve`)
- CLI commands for querying, reporting, and data exploration remain fully functional

**⚠️ Query Column Renames**
- Database schema v10 renames `file_size` → `size` and `total_file_size` → `total_size` to reflect directory size support
- Queries using old column names will fail after upgrade

**⚠️ Query Language Updates**
- Computed columns (`adds`, `modifies`, `deletes`) replaced with stored columns (`add_count`, `modify_count`, `delete_count`)
- Queries using old column names will fail after upgrade

### Added

**🗓️ Scheduled and Recurring Scans**
- New scheduling system with daily, weekly, monthly, and interval-based automatic scans
- Queue-based execution with database-backed persistence

**📁 Browse Page with Item Detail View**
- Navigate filesystem hierarchy with detailed item cards showing metadata, validation status, change history, and alerts
- Elegant sliding panel interface for item inspection

**📊 Scan Trends Visualization**
- New Insights tab with interactive charts showing historical scan data
- Track file size, file/folder counts, change activity, and alerts over time
- Features root selection, date range filtering, and smart baseline exclusion

**💾 Folder Size Calculation**
- Folder sizes now computed during scan and stored in database
- Dual-format display (decimal and binary units): e.g., "16.3 MB (15.54 MiB)"

**🎯 Enhanced Scan Statistics**
- Denormalized count columns in scans table for improved query performance
- Home page displays total file size and color-coded change indicators

**🔍 UI Enhancements**
- Unified filter toolbar design across Browse and Alerts pages
- Path search with debouncing on Browse page
- Standalone Alerts page with context filtering
- Preset time window selector with quick-select ranges

### Changed

**⚛️ Complete React Migration**
- Replaced 5,800-line HTML template with React 19 + shadcn/ui
- Improved responsiveness, accessibility, and maintainability

**📦 Single-Binary Distribution**
- Assets embedded using rust-embed with conditional compilation
- Development builds serve from filesystem; release builds embed assets in binary

**🎨 UI Design Language Overhaul**
- Card-based layouts with refined typography and spacing
- Consistent component styling across all pages

**🔧 Progress Reporting Simplification**
- Consolidated from 3 files to 1 with minimal 14-method API
- Validators now pure validation functions; Scanner tracks progress

**📂 Recursive Directory Scanning**
- Replaced queue-based traversal with depth-first recursive scanning
- Enables bottom-up folder size calculation

**🔢 Natural Path Sorting**
- Database schema v8 implements natural, case-insensitive path sorting
- Hierarchical ordering (e.g., `/proj` before `/proj-A`) with proper numeric handling

**⚡ Integer-Based Enum Storage**
- Database schema v7 migrates enums to integer values for type safety and performance
- ChangeType reordered to logical sequence (NoChange=0, Add=1, Modify=2, Delete=3)

**🗄️ Standardized Transaction Pattern**
- All transactions now use IMMEDIATE mode for consistency and safety

**🖥️ Chart and Visualization Improvements**
- Standardized chart titles, improved visualization types
- Integer-only Y-axes for count-based charts

### Fixed

- Activity and Monitor page table refresh with proper loading states
- Monitor page button states (Add Schedule always enabled; Delete disabled during active scan)
- Root deletion now properly removes associated schedules and queue entries
- Alert path formatting using correct `@name` format specifier
- ItemDetailSheet alerts loading corrected to use `val_error` column
- Tombstone exclusion in file/folder counts
- Schema migration corrections for ValidationState and enum mappings
- Null date display in Explore view

## [v0.1.4] - 2025-10-25

### Added

- **Error tracking for scans**: Database schema v5 adds error field to scans table with automatic migration from v4
- **Error state**: New scan state for failed scans, distinct from Stopped state for user-cancelled scans
- **Error handling**: Failed scans now rollback database changes and store error messages, visible in CLI reports, Web UI scan cards, Home page stats, and query results

### Changed

- **Web UI Scans page redesign**: New layout with scan action buttons, schedule placeholders, and improved table styling

## [v0.1.3] - 2025-10-24

### Changed

- Update dependencies to latest minor versions: console 0.16, dialoguer 0.12, flexi_logger 0.31, indicatif 0.18, phf 0.13, rusqlite 0.37, tabled 0.20, tokio 1.48, toml 0.9
- Fix tabled API compatibility: `Columns::single()` → `Columns::one()`
- **CI/CD modernization**: Restructured GitHub workflows following industry best practices - reusable test workflow eliminates duplication, cargo caching reduces build times, artifact retention policies reduce storage costs, and sequential release job eliminates race conditions
- **macOS Apple Silicon support**: Release artifacts now include native ARM64 builds for Apple Silicon Macs (M1/M2/M3/M4/M5) alongside Intel builds
- **Enhanced release.sh script**: Added comprehensive safety checks - branch validation, working tree verification, remote sync check, tag existence check, atomic push with rollback, and cross-platform compatibility

## [v0.1.2] - 2025-10-23

### Changed

- **Web UI Home page enhancements**: Enhanced Home page (previously Overview) with live scan statistics display showing real-time progress for active scans and detailed statistics for completed scans
- **Improved scan state visibility**: Added comprehensive state management for scans including in-progress, incomplete, stopped, and completed states with appropriate user guidance
- **UI refinements**: Updated navigation terminology ("Scans" → "Scan") and icons (magnifying glass for Scan, database icon for Explore) for clearer user intent
- **Backend optimizations**: Added ScanStats aggregation for efficient statistics queries with breakdowns by change type and item type

## [v0.1.1] - 2025-10-23

### Fixed

- **Database directory resolution**: Removed automatic `/data` directory detection that could incorrectly use `/data` on non-Docker systems if `/data/config.toml` happened to exist. Docker containers explicitly set `FSPULSE_DATA_DIR=/data`, making the auto-detect redundant and potentially incorrect.
- **CI/Docker workflow triggers**: Added `README.md` to paths-ignore to prevent unnecessary workflow runs for documentation-only changes

## [v0.1.0] - 2025-10-22

### Breaking Changes

**⚠️ CLI Database Parameter Removed**
- Removed the `--db` / `-d` CLI parameter
- Database location is now managed through configuration system
- **Migration options** (in order of precedence):
  1. Environment variable: `FSPULSE_DATABASE_PATH=/path/to/db.sqlite`
  2. Config file: `[database].path = "/path/to/db.sqlite"` in `~/.config/fspulse/config.toml`
  3. Default location if neither is specified

**⚠️ Default Scan Behavior Changed**
- Hash default is now "hash all" - all items will be hashed by default
- Validate new/changed is now the default validation mode
- Override options: `--no-hash`, `--hash-new`, `--no-validate`, `--validate-all`

### Added

**🚀 Web UI and Server Mode**
- New `serve` command launches FsPulse as a web server with full-featured UI
- Real-time scan monitoring with WebSocket updates
- Interactive data exploration with dynamic filtering and column management
- Alert management interface with context-aware filtering
- Query builder with support for all FsPulse query syntax
- Configurable via environment variables or `[server]` section in config:
  - `FSPULSE_SERVER_HOST` / `[server].host` (default: 127.0.0.1)
  - `FSPULSE_SERVER_PORT` / `[server].port` (default: 8080)

**🐳 Docker Support**
- Official Docker images now available at `gtunesdev/fspulse`
- Multi-architecture support (linux/amd64, linux/arm64)
- Automated builds triggered by version tags
- Tagged releases: `latest`, `0.1.0`, `0.1`

**📊 Alerts System**
- New top-level data type for tracking integrity issues
- Automatically generated during scan analysis phase
- Two alert types: Suspicious Hash changes, Invalid Items
- Alert status management (Open, Flagged, Dismissed)
- Exposed in query model as `alerts` domain
- Alerts tab in Explore view with automatic filtering

**🎨 Enhanced Explorer UI**
- Collapsible filter frame (Ctrl+F to toggle)
- Views system: Press 'V' for pre-configured view templates
- View persistence per data type
- Improved column ordering and management

**📝 Query Enhancements**
- Added `@timestamp` format modifier for dates (UTC Unix timestamps)
- Enables client-side timezone conversion in web applications
- All schema fields now available for querying (e.g., `is_undelete`, `last_hash_scan_old`, `hash_old`, `hash_new`, `last_val_scan_old`)

**⚙️ Environment Variable Configuration**
- All configuration options now support environment variable overrides
- Environment variables take precedence over config file values
- Naming pattern: `FSPULSE_<SECTION>_<KEY>` (e.g., `FSPULSE_DATABASE_PATH`, `FSPULSE_SERVER_PORT`)
- Enables easier Docker and CI/CD configuration

### Changed

- Moved all input boxes from tui-input to tui-textarea for better cursor display and text box behaviors
- Improved navigation architecture in web UI with consistent flexbox layout
- Updated responsive breakpoint from 768px to 480px for better tablet support
- Consolidated overflow handling to single source of truth

### Fixed

- Windows: Filtered out key release events that caused double-press behavior
- Navigation sidebar expansion now works consistently across all web UI pages
- Status dropdown display issues resolved with proper column constraints
- Alert status updates now persist with correct UTC timestamps

## [v0.0.14] - 2025-05-07

### Fixed

- On Windows, key release events weren't filtered out so a single key press
  and release appeared as double-press events

## [v0.0.12] - 2025-05-07

### Added

- Explicit ordering in Explorer! The column view now supports pressing 'a' for
  "ascending" and 'd' for "descending. It also supports left and right arrow for
  cycling through the options. If you want to order on multiple columns, 
  those columns must be displayed in the order that you want the directives 
  applied. Use '+' and '-' to re-order columns. If order is specified on a 
  hidden column, that order is ignored. 

- Some schematized fields weren't available for query in CLI or Explore. All fields
  are now available. New fields include is_undelete, last_hash_scan_old
  hash_old, hash_new, last_val_scan_old

- Explore columns now use shortened display names which are acronyms in the case
  of very long field names. For example, last_val_scan_old is LVSO

- Documentation update to include all available schema fields in the "book"

### Fixed

- Corrected "not_null" to "not null" in Explorer filter tips

- In Explorer, date filter values are validated before the filter is saved.
  Previously, dates were parsed but not actually validated so values such as
  "2025-01-32" would slip past the parser and into the query

- Fixed panic occurring when the window width was too small to display the
  vertical scrollbar in the grid

### Changed

- Cleaned up StringFilter - enum types have had their own filter type for
  a while but StringFilter still had legacy enum validation logic

- Internal cleanup of Explore's "column" data structures. Collapsed
  ColumnOption and ColumnInfo

- Grid rows are no longer cloned prior to drawing the table. The rows
  are re-created (no way to avoid this while using table state) but the
  String contents are no longer cloned

## [v0.0.11] - 2025-05-05

### Added

- New feature: Explore. This is a complete terminal-UI experience for exploring
  FsPulse data. View Items, Changes, Scans, Roots. Create and modify query filters.
  Show and hide columns. Implemented with Ratatui. More coming soon!

## [v0.0.10] - 2025-04-26

No code changes in this release - just pushing out a couple of changes to address
issues with recommended queries.

### Fixed

- A couple of additional issues with queries recommended after scans


## [v0.0.9] - 2025-04-24

### Fixed

- Fixed issue with string property definitions in query grammar which case val_error_old and
  val_error_new to be unrecognized

- Fixed issues with recommended queries in scan reports

## [v0.0.8] - 2025-04-23

### Changed

- Fix a few missed references to MD5 in doc book and CLI help

## [v0.0.7] - 2025-04-23

### Changed

- The default hashing function has been changed to SHA2. This will cause previously hashed 
  files to appear changed due to the different algorithm. If you really
  want to stick with MD5, you can set the hash function via config, which is detailed
  in documentation.

### Added

- Config [analysis]\hash with allowable values of "md5" and "sha2". If not specified, "sha2" will be used.
  Note: see documentation on config.

## [v0.0.6] - 2025-04-23

### Fixed

- Fix remaining file count for analysis phase


## [v0.0.5] - 2025-04-22

### Fixed

- SQL query bug where tombstones were incorrectly included in hash/validation candidate sets
- Corrected hash/validate progress logic for newly added items

### Changed

- Improved release process with `release.sh` and GitHub CI integration
- Cleaned up `clap` comments and help output formatting


## [v0.0.4] - 2025-04-20
### Added
- Interactive query mode with readline history
- Colorized validation summaries using `console`

### Changed

- Refined table alignment and dynamic column display via `tabled`
- Replaced manual match strings with `phf_ordered_map!` for column specs


## [v0.0.3] - 2025-04-15

### Added

- `--validate` and `--hash` CLI flags
- Structured query parsing with Pest PEG grammar
- Query domains: `roots`, `scans`, `items`, and `changes`

## [v0.0.2] - 2025-04-10

### Added

- SQLite-backed schema with version tracking
- Change tracking using tombstones and generation-based scanning

### Changed

- Eliminated inode usage to improve portability
- Switched to breadth-first scan with `VecDeque`

## [v0.0.1] - 2025-04-01

### Added
- Initial CLI scaffold with `clap`
- Basic scan and record of directory metadata
- Schema support for tracking file adds, deletes, and modifies