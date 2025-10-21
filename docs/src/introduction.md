# Introduction

<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" style="width: 100%; max-width: 600px;">

FsPulse is a Rust-based tool designed to capture and analyze directory states, detect changes over time, validate file integrity and formats, and query results using a powerful and intuitive query syntax. It runs as a **web service** with browser-based interface, or as a traditional **command-line tool**.

## Key Capabilities

- **Web UI & Background Service** — Run as a daemon with browser-based access to all features
- **Command-Line Interface** — Full CLI with interactive TUI modes for terminal users
- **Directory Scanning** — Track additions, deletions, and modifications of files and directories
- **Content Validation** — Validate file formats such as FLAC, JPEG, GIF, BMP, PNG, TIFF, and PDF
- **SHA2 Hashing** — Optionally detect file content changes beyond metadata
- **Powerful Querying** — Access scan results with flexible, SQL-like queries via web UI or CLI
- **Docker Deployment** — Official Docker images for easy containerized deployment

## Usage Modes

FsPulse offers flexibility in how you interact with it:

- **Server Mode**: Run `fspulse serve` to start a web server on port 8080, providing full functionality through your browser
- **CLI Mode**: Use `fspulse scan`, `fspulse query`, and other commands for scriptable, terminal-based workflows
- **Interactive TUI**: Use `fspulse interact` or `fspulse explore` for menu-driven exploration in the terminal

FsPulse is designed to scale across large file systems while maintaining clarity and control for the user.
