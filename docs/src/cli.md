# Command-Line Interface

FsPulse is a **web-first application**. The CLI exists solely to launch the web server—all functionality including scanning, querying, browsing, and configuration is accessed through the [Interface](web_ui.md).

---

## Starting FsPulse

To start the FsPulse server:

```sh
fspulse
```

Or explicitly:

```sh
fspulse serve
```

Both commands are equivalent. The server starts on `http://127.0.0.1:8080` by default.

Once running, open your browser to access the full web interface for:
- Managing scan roots and schedules
- Running and monitoring scans
- Browsing your filesystem data
- Querying and exploring results
- Managing alerts

---

## Configuration

FsPulse behavior is configured through **environment variables** or a **config file**, not command-line flags.

### Environment Variables

Set these before running `fspulse`:

```sh
# Server settings
export FSPULSE_SERVER_HOST=0.0.0.0    # Bind address (default: 127.0.0.1)
export FSPULSE_SERVER_PORT=9090       # Port number (default: 8080)

# Analysis settings
export FSPULSE_ANALYSIS_THREADS=16    # Worker threads (default: 8)

# Logging
export FSPULSE_LOGGING_FSPULSE=debug  # Log level (default: info)

# Data location
export FSPULSE_DATA_DIR=/custom/path  # Data directory override

fspulse
```

### Configuration File

FsPulse also reads from `config.toml` in the data directory. See [Configuration](configuration.md) for complete documentation including:
- All available settings
- Environment variable reference
- Platform-specific data directory locations
- Docker configuration

---

## Getting Help

View version and basic usage:

```sh
fspulse --help
fspulse --version
```

---

## Related Documentation

- **[Configuration](configuration.md)** — Complete configuration reference
- **[Interface](web_ui.md)** — Guide to all UI features
- **[Docker Deployment](docker.md)** — Running FsPulse in Docker

