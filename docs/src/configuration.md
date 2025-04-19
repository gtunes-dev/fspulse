# Configuration

FsPulse supports persistent, user-defined configuration through a file named `config.toml`. This file allows you to control logging behavior and analysis settings such as thread usage.

---

## Finding `config.toml`

FsPulse uses the [directories](https://docs.rs/directories) crate to determine the appropriate location for configuration files based on your operating system.

### Where it's stored:

| Platform | Base Location         | Example             |
|----------|------------------------|---------------------|
| Linux    | `$HOME`               | `/home/alice`       |
| macOS    | `$HOME`               | `/Users/Alice`      |
| Windows  | `{FOLDERID_Profile}`  | `C:\Users\Alice`   |

On the first run, if no `config.toml` is found, FsPulse will automatically create one with default settings appropriate for your platform.

> Tip: You can delete `config.toml` at any time to regenerate it with defaults. Newly introduced settings will not automatically be added to an existing file.

---

## Configuration Settings

Here are the current available settings and their default values:

```toml
[logging]
fspulse = "info"
lopdf = "error"

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

- Logs are written to a `logs/` folder in the same directory as `config.toml`
- Each run of FsPulse creates a new log file, named using the current date and time
- FsPulse retains up to **100** log files; older files are automatically deleted

---

## Analysis Settings

The `[analysis]` section controls how many threads are used during the **analysis phase** of scanning (for hashing and validation).

- `threads`: number of worker threads (default: `8`)

You can adjust this based on your system's CPU count or performance needs.

---

## New Settings and Restoring Defaults

FsPulse may expand its configuration options over time. When new settings are introduced, they won't automatically appear in your existing `config.toml`. To take advantage of new options, either:

- Manually add new settings to your config file
- Delete the file to allow FsPulse to regenerate it with all current defaults

