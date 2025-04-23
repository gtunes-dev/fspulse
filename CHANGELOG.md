# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [0.0.5] - 2025-04-22
### Fixed
- SQL query bug where tombstones were incorrectly included in hash/validation candidate sets
- Corrected hash/validate progress logic for newly added items

### Changed
- Improved release process with `release.sh` and GitHub CI integration
- Cleaned up `clap` comments and help output formatting

---

## [0.0.4] - 2025-04-20
### Added
- Interactive query mode with readline history
- Colorized validation summaries using `console`

### Changed
- Refined table alignment and dynamic column display via `tabled`
- Replaced manual match strings with `phf_ordered_map!` for column specs

---

## [0.0.3] - 2025-04-15
### Added
- `--validate` and `--hash` CLI flags
- Structured query parsing with Pest PEG grammar
- Query domains: `roots`, `scans`, `items`, and `changes`

---

## [0.0.2] - 2025-04-10
### Added
- SQLite-backed schema with version tracking
- Change tracking using tombstones and generation-based scanning

### Changed
- Eliminated inode usage to improve portability
- Switched to breadth-first scan with `VecDeque`

---

## [0.0.1] - 2025-04-01
### Added
- Initial CLI scaffold with `clap`
- Basic scan and record of directory metadata
- Schema support for tracking file adds, deletes, and modifies