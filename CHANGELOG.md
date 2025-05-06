# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [v0.0.12] - Not Yet Released

### Fixed

- Corrected "not_null" to "not null" in Explorer filter tips
- In Explorer, date filter values are validated before the filter is saved.
  Previously, dates were parsed but not actually validated so values such as
  "2025-01-32" would slip past the parser and into the query

### Changed

- Cleaned up StringFilter - enum types have had their own filter type for
  a while but StringFilter still had legacy enum validation logic

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