# Configuration

FsPulse supports persistent, user-defined configuration through a file named `config.toml`. This file allows you to control logging behavior, analysis settings, server configuration, and more.

> **📦 Docker Users**: If you're running FsPulse in Docker, see the [Docker Deployment](docker.md) chapter for Docker-specific configuration including environment variable overrides and volume management.

---

## Finding `config.toml`

The location of `config.toml` depends on how you're running FsPulse:

### Docker Deployments

When running in Docker, the config file is located at **`/data/config.toml`** inside the container. FsPulse automatically creates this file with default settings on first run.

To access it from your host machine:
```bash
# View the config
docker exec fspulse cat /data/config.toml

# Extract to edit
docker exec fspulse cat /data/config.toml > config.toml
```

See the [Docker Deployment](docker.md#configuration) chapter for details on editing the config in Docker.

### Native Installations

FsPulse uses the [directories](https://docs.rs/directories) crate to determine platform-specific locations:

| Platform | Location Description     | Example Path                                                  |
|----------|---------------------------|---------------------------------------------------------------|
| Linux    | `$XDG_DATA_HOME`          | `/home/alice/.local/share/fspulse`                           |
| macOS    | Application Support       | `/Users/alice/Library/Application Support/fspulse`           |
| Windows  | Local AppData             | `C:\Users\Alice\AppData\Local\fspulse`                   |

On first run, FsPulse automatically creates this directory and writes a default `config.toml` if one doesn't exist.

> **Tip**: You can delete `config.toml` at any time to regenerate it with defaults. Newly introduced settings will not automatically be added to an existing file.

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
hash = "sha2"
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

- Logs are written to a `logs/` folder inside the same local data directory as `config.toml`
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

You can adjust this based on your system's CPU count or performance needs.

- `hash`: hash function to use when hashing files. Values can be `sha2` or `md5` (default: `sha2`)

Sha2 is more secure but is slower. It is appropriate for most users.

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

Configure scan behavior, hashing, and validation:

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_ANALYSIS_THREADS` | `8` | 1-256 | Number of worker threads for analysis phase (hashing/validation) |
| `FSPULSE_ANALYSIS_HASH` | `sha2` | `sha2`, `md5` | Hash algorithm (sha2 is more secure, md5 is faster) |

**Examples:**
```bash
# Use 16 threads with MD5 hashing
export FSPULSE_ANALYSIS_THREADS=16
export FSPULSE_ANALYSIS_HASH=md5

# Docker
docker run -e FSPULSE_ANALYSIS_THREADS=16 -e FSPULSE_ANALYSIS_HASH=md5 ...
```

#### Database Settings

Control database location (advanced):

| Variable | Default | Valid Values | Description |
|----------|---------|--------------|-------------|
| `FSPULSE_DATA_DIR` | Platform-specific | Directory path | Override entire data directory location |
| `FSPULSE_DATABASE_PATH` | `<data_dir>` | Directory path | Override database directory (rarely needed) |

**Note**: Most users should use `FSPULSE_DATA_DIR` rather than `FSPULSE_DATABASE_PATH`. In Docker, this defaults to `/data`.

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

