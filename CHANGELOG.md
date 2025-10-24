# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Update dependencies to latest minor versions: console 0.16, dialoguer 0.12, flexi_logger 0.31, indicatif 0.18, phf 0.13, rusqlite 0.37, tabled 0.20, tokio 1.48, toml 0.9
- Fix tabled API compatibility: `Columns::single()` → `Columns::one()`
- **CI/CD modernization**: Restructured GitHub workflows following industry best practices - reusable test workflow eliminates duplication, cargo caching reduces build times, artifact retention policies reduce storage costs, and sequential release job eliminates race conditions
- **macOS Apple Silicon support**: Release artifacts now include native ARM64 builds for Apple Silicon Macs (M1/M2/M3/M4/M5) alongside Intel builds

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