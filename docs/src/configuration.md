# Configuration

FsPulse supports persistent, user-defined configuration through a file named `config.toml`. This file allows you to control logging behavior, analysis settings, server configuration, and more.

> **Web UI**: Most configuration settings can also be viewed and edited through the [Settings](web_ui/settings.md) page in the web interface, which shows the active value and its source (default, config file, or environment variable).

> **📦 Docker Users**: If you're running FsPulse in Docker, see the [Docker Deployment](docker.md) chapter for Docker-specific configuration including environment variable overrides and volume management.

---

## Finding `config.toml`

The `config.toml` file is stored in FsPulse's **data directory**. The location depends on how you're running FsPulse:

### Docker Deployments

When running in Docker, the data directory is **`/data`**, so the config file is located at **`/data/config.toml`** inside the container. FsPulse automatically creates this file with default settings on first run.

To access it from your host machine:
```bash
# View the config
docker exec fspulse cat /data/config.toml

# Extract to edit
docker exec fspulse cat /data/config.toml > config.toml
```

See the [Docker Deployment](docker.md#configuration) chapter for details on editing the config in Docker.

### Native Installations

FsPulse uses the [directories](https://docs.rs/directories) crate to determine the platform-specific data directory location:

| Platform | Data Directory Location | Example Path                                      |
|----------|-------------------------|---------------------------------------------------|
| Linux    | `$XDG_DATA_HOME/fspulse` or `$HOME/.local/share/fspulse` | `/home/alice/.local/share/fspulse`   |
| macOS    | `$HOME/Library/Application Support/fspulse`             | `/Users/alice/Library/Application Support/fspulse` |
| Windows  | `%LOCALAPPDATA%\fspulse\data`                           | `C:\Users\Alice\AppData\Local\fspulse\data`       |

The config file is located at `<data_dir>/config.toml`.

On first run, FsPulse automatically creates the data directory and writes a default `config.toml` if one doesn't exist.

> **Tip**: You can delete `config.toml` at any time to regenerate it with defaults. Newly introduced settings will not automatically be added to an existing file.
>
> **Override**: The data directory location can be overridden using the `FSPULSE_DATA_DIR` environment variable. See [Data Directory and Database Settings](#data-directory-and-database-settings) for details.

---

## Configuration Settings

Here are the current available settings and their default values:

```toml
[logging]
fspulse = "info"
lopdf = "error"

[server]
port = 8080
host = "127.0.0.1"

[analysis]
threads = 8
```

---

## Logging

FsPulse uses the Rust [`log`](https://docs.rs/log) crate, and so does the PDF validation crate `lopdf`. You can configure logging levels independently for each subsystem in the `[logging]` section.

### Supported log levels:

- `error` – only critical errors
- `warn` – warnings and errors
- `info` – general status messages (default for FsPulse)
- `debug` – verbose output for debugging
- `trace` – extremely detailed logs

### Log File Behavior

- Logs are written to `<data_dir>/logs/`
- Each run of FsPulse creates a new log file, named using the current date and time
- FsPulse retains up to **100** log files; older files are automatically deleted

---

## Server Settings

The `[server]` section controls the web UI server behavior when running `fspulse serve`.

- `host`: IP address to bind to (default: `127.0.0.1`)
  - `127.0.0.1` - Localhost only (secure, only accessible from same machine)
  - `0.0.0.0` - All interfaces (required for Docker, remote access)
- `port`: Port number to listen on (default: `8080`)

**Note**: In Docker deployments, the host should be `0.0.0.0` to allow access from outside the container. The Docker image sets this automatically via environment variable.

---

## Analysis Settings

The `[analysis]` section controls how many threads are used during the **analysis phase** of scanning (for hashing and validation).

- `threads`: number of worker threads (default: `8`)

You can adjust this based on your system's CPU count or performance needs. FsPulse uses SHA-256 for file hashing to detect content changes and verify integrity.

---

## Environment Variables

All configuration settings can be overridden using environment variables. This is particularly useful for:
- **Docker deployments** where editing files is inconvenient
- **Different environments** (development, staging, production) with different settings
- **NAS deployments** (TrueNAS, Unraid) using web-based configuration UIs
- **CI/CD pipelines** where configuration is managed externally

### How It Works

Environment variables follow the pattern: **`FSPULSE_<SECTION>_<FIELD>`**

The `<SECTION>` corresponds to a section in `config.toml` (like `[server]`, `[logging]`, `[analysis]`), and `<FIELD>` is the setting name within that section.

**Precedence** (highest to lowest):
1. **Environment variables** - Override everything
2. **config.toml** - User-defined settings
3. **Built-in defaults** - Fallback values

This allows you to set sensible defaults in `config.toml` and override them as needed per deployment.

### Complete Variable Reference

#### Server Settings

Control the web UI server behavior (when running `fspulse serve`):

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_SERVER_HOST` | `127.0.0.1` | IP address | Bind address. Use `0.0.0.0` for Docker/remote access, `127.0.0.1` for localhost only |
| `FSPULSE_SERVER_PORT` | `8080` | 1-65535 | Web UI port number |

**Examples:**
```bash
# Native - serve only on localhost
export FSPULSE_SERVER_HOST=127.0.0.1
export FSPULSE_SERVER_PORT=8080
fspulse serve

# Docker - must bind to all interfaces
docker run -e FSPULSE_SERVER_HOST=0.0.0.0 -e FSPULSE_SERVER_PORT=9090 -p 9090:9090 ...
```

#### Logging Settings

Configure log output verbosity:

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_LOGGING_FSPULSE` | `info` | `error`, `warn`, `info`, `debug`, `trace` | FsPulse application log level |
| `FSPULSE_LOGGING_LOPDF` | `error` | `error`, `warn`, `info`, `debug`, `trace` | PDF library (lopdf) log level |

**Examples:**
```bash
# Enable debug logging
export FSPULSE_LOGGING_FSPULSE=debug
export FSPULSE_LOGGING_LOPDF=error

# Docker
docker run -e FSPULSE_LOGGING_FSPULSE=debug ...
```

#### Analysis Settings

Configure scan behavior and performance:

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_ANALYSIS_THREADS` | `8` | 1-24 | Number of worker threads for analysis phase (hashing/validation) |

**Examples:**
```bash
# Use 16 threads for faster scanning
export FSPULSE_ANALYSIS_THREADS=16

# Docker
docker run -e FSPULSE_ANALYSIS_THREADS=16 ...
```

#### Data Directory and Database Settings

Control where FsPulse stores its data:

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_DATA_DIR` | Platform-specific | Directory path | Override the data directory location. Contains config, logs, and database (by default). Cannot be set in config.toml. |
| `FSPULSE_DATABASE_DIR` | `<data_dir>` | Directory path | Override database directory only (advanced). Stores the database outside the data directory. This is a directory path, not a file path - the database file is always named `fspulse.db` |

**Data Directory:**

The data directory contains configuration (`config.toml`), logs (`logs/`), and the database (`fspulse.db`) by default. It is determined by:

1. `FSPULSE_DATA_DIR` environment variable (if set)
2. Platform-specific project local directory (default):
   - **Linux**: `$XDG_DATA_HOME/fspulse` or `$HOME/.local/share/fspulse`
   - **macOS**: `$HOME/Library/Application Support/fspulse`
   - **Windows**: `%LOCALAPPDATA%\fspulse\data`
   - **Docker**: `/data`

**Database Location:**

By default, the database is stored in the data directory as `fspulse.db`. You can override this to store the database separately:

**Database Directory Precedence:**
1. `FSPULSE_DATABASE_DIR` environment variable (if set) - highest priority
2. `config.toml` `[database]` `dir` setting (if configured)
3. Data directory (from `FSPULSE_DATA_DIR` or platform default)

**Important Notes:**
- The database file is always named **`fspulse.db`** within the determined directory
- Configuration and logs always remain in the data directory, even if the database is moved
- For Docker: it's recommended to use volume/bind mounts to `/data` rather than overriding `FSPULSE_DATA_DIR`

#### Docker-Specific Variables

These variables are specific to Docker deployments:

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `PUID` | `1000` | UID number | User ID to run FsPulse as (for NAS permission matching) |
| `PGID` | `1000` | GID number | Group ID to run FsPulse as (for NAS permission matching) |
| `TZ` | `UTC` | Timezone string | Timezone for log timestamps and UI (e.g., `America/New_York`) |

See [Docker Deployment - NAS Deployments](docker.md#nas-deployments-truenas-unraid) for details on PUID/PGID usage.

### Usage Examples

**Native (Linux/macOS/Windows):**
```bash
# Set environment variables
export FSPULSE_SERVER_PORT=9090
export FSPULSE_LOGGING_FSPULSE=debug
export FSPULSE_ANALYSIS_THREADS=16

# Run FsPulse (uses env vars)
fspulse serve
```

**Docker - Command Line:**
```bash
docker run -d \
  --name fspulse \
  -e FSPULSE_SERVER_PORT=9090 \
  -e FSPULSE_LOGGING_FSPULSE=debug \
  -e FSPULSE_ANALYSIS_THREADS=16 \
  -p 9090:9090 \
  -v fspulse-data:/data \
  gtunesdev/fspulse:latest
```

**Docker Compose:**
```yaml
services:
  fspulse:
    image: gtunesdev/fspulse:latest
    environment:
      - FSPULSE_SERVER_PORT=9090
      - FSPULSE_LOGGING_FSPULSE=debug
      - FSPULSE_ANALYSIS_THREADS=16
    ports:
      - "9090:9090"
```

### Verifying Environment Variables

To see what environment variables FsPulse is using:

**Native:**
```bash
env | grep FSPULSE_
```

**Docker:**
```bash
docker exec fspulse env | grep FSPULSE_
```

---

## Docker Configuration

When running FsPulse in Docker, configuration is managed slightly differently. The config file lives at `/data/config.toml` inside the container, and you have several options for customizing settings.

For step-by-step instructions on configuring FsPulse in Docker, including editing config files and using environment variables, see the [Docker Deployment - Configuration](docker.md#configuration) section.

---

## New Settings and Restoring Defaults

FsPulse may expand its configuration options over time. When new settings are introduced, they won't automatically appear in your existing `config.toml`. To take advantage of new options, either:

- Manually add new settings to your config file
- Delete the file to allow FsPulse to regenerate it with all current defaults

