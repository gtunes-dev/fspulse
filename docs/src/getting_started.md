# Getting Started

## Building from Source

To build FsPulse from source:

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
```

You can then run the binary from the `target/release` directory or move it to a directory included in your system's `PATH`:

```sh
./target/release/fspulse --help
```

## First Scan

To scan a directory:

```sh
fspulse scan --root-path /some/directory
```

## Interactive Exploration

After scanning, you can explore results in an interactive shell:

```sh
fspulse interact
```

## Querying

Use flexible, SQL-like queries to retrieve and filter scan results:

```sh
# Items whose path contains 'reports'
fspulse query "items where item_path:('reports')"

# Changes involving items detected as invalid
fspulse query "changes where val_new:(I) show default, val_old, val_new order by change_id desc"
```

See the [Query Syntax](query.md) page for details.
