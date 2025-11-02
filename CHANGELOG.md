# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Browse page with detailed item view**: New Browse page allows navigation through filesystem hierarchy with detailed item cards showing file metadata, validation status, change history, and associated alerts in an elegant sliding panel interface
- **Enhanced scan statistics**: Database schema v6 adds denormalized count columns to scans table (`total_file_size`, `alert_count`, `add_count`, `modify_count`, `delete_count`) for improved query performance and future charting capabilities. total_file_size will be computed for new scans only
- **Home page statistics display**: Added total file size and aggregate change count displays with color-coded visual indicators for adds (green), modifies (blue), and deletes (red)
- **Scan Trends visualization**: New Insights tab with interactive charts showing historical scan data over time, including total file size, file/folder counts, change activity (adds/modifies/deletes), and alerts created. Features root selection, date range filtering, and human-readable formatting for large numbers and byte sizes
- **Standalone Alerts page**: Moved Alerts from Insights tabs to dedicated top-level navigation page with context filtering by root or scan ID
- **Preset time window selector**: Added quick-select time ranges (Last 7 Days, Last 30 Days, Last 3 Months, Last 6 Months, Last Year, Custom Range) with inline custom date pickers
- **Smart baseline scan filtering**: Added "Exclude initial baseline scan" checkbox for Changes and New Alerts charts that automatically detects and filters the first scan (or first validating scan for alerts) when present in the time window

### Changed

- **Natural path sorting**: Database schema v8 implements natural, case-insensitive path sorting using the icu_collator crate. Paths now sort hierarchically (e.g., `/proj` and its children appear before `/proj-A`) with proper numeric ordering (e.g., `file2` before `file10`). Updated all queries and indexes to use the natural_path collation
- **Integer-based enum storage**: Database schema v7 migrates all enums (item_type, change_type, alert_type, alert_status, validation state, scan_state) from single-character string storage to integer values for improved type safety and performance
- **ChangeType enum reordering**: Changed ChangeType integer values to logical order (NoChange=0, Add=1, Modify=2, Delete=3) and updated all SQL queries, documentation, and tests accordingly
- **Removed Default trait from enums**: Eliminated Default implementations from all enums to enforce explicit value handling
- **Removed ScanState::Unknown variant**: Eliminated invalid Unknown state from ScanState enum
- **Query column rename**: Renamed `state` column to `scan_state` in scans query domain
- **Query language updates**: Replaced computed columns (`adds`, `modifies`, `deletes`) with stored columns (`add_count`, `modify_count`, `delete_count`) in scans queries - **Breaking change**: queries using old column names will fail after upgrade
- **Complete React migration**: Replaced monolithic 5,800-line HTML template with modern React 19 + shadcn/ui component library, featuring improved responsiveness, accessibility, and maintainability
- **Single-binary distribution**: Implemented embedded assets using rust-embed with conditional compilation - development builds serve from filesystem for fast iteration, release builds embed assets into binary for simplified deployment
- **Build infrastructure updates**: Updated GitHub CI workflows and Dockerfile to build frontend before Rust compilation, ensuring embedded assets are included in release artifacts
- **Web UI Scan page fit and finish**: Improved layout and spacing of Roots card header with repositioned "Add Root" button
- **Chart visualization improvements**: Standardized chart titles to singular form, converted Items chart to stacked area visualization, Changes chart to stacked bars (single bar per scan), and New Alerts to bar chart for better clarity of discrete events
- **Insights page redesign**: Improved visual hierarchy with prominent root selector and compact secondary time range controls, automatic selection of first root with 3-month default time range
- **Integer-only Y-axes**: All count-based charts (Items, Changes, New Alerts) now enforce integer tick marks, preventing misleading decimal values

### Fixed

- **Tombstone exclusion**: Corrected `file_count` and `folder_count` computation to exclude tombstoned (deleted) items, fixing a bug where deleted items were incorrectly included in totals
- **Schema migration corrections**: Fixed v6_to_v7 migration to use correct integer mappings for ValidationState, and corrected v5_to_v6 migration to use character values (not integers) when operating on pre-v7 database
- **Invalid enum value logging**: Added warning logs when database contains invalid enum integer values, helping detect data corruption or migration issues while maintaining graceful degradation
- **Comprehensive enum tests**: Added integer value and round-trip conversion tests for all enum types to prevent future mapping errors
- **Null date display in Explore view**: Fixed DataExplorerView to check for null sentinel value ("-") before attempting to parse date columns, preventing "NaN-NaN-NaN" display for null dates

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