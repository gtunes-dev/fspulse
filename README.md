<img src=\"https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png\" alt=\"banner with image of folder and magnifying glass\">

# FsPulse

FsPulse is a Rust-based command-line tool designed to capture and analyze directory states, detect changes over time, validate file integrity and formats, and query results using a powerful and intuitive query syntax.

## Key Capabilities

- **Directory Scanning**: Track additions, deletions, and modifications of files and directories.
- **Content Validation**: Validate file formats (FLAC, JPEG, GIF, BMP, PDF).
- **MD5 Hashing**: Optionally detect file content changes beyond metadata.
- **Powerful Querying**: Access scan results directly with flexible, SQL-like queries.
- **Interactive Mode**: Easily navigate and explore scan results interactively after performing a scan.

## 📚 Documentation

Detailed documentation, including installation, usage examples, query syntax, and development guides, is available here:

👉 **[FsPulse Documentation](https://gtunes-dev.github.io/fspulse/)** *(link coming soon!)*

- 📖 [Query Syntax Documentation](https://gtunes-dev.github.io/fspulse/query.html) *(link coming soon!)*

---

## Building from Source

To build FsPulse from source, clone the repository and build with Cargo:

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
```

You can then run the binary from the `target/release` directory or move it to a directory included in your system's `PATH`.

```sh
./target/release/fspulse --help
```

## Quick Example

Run a basic scan:

```sh
fspulse scan --root-path /some/directory
```

Interactively explore the results of your scans:

```sh
fspulse interact
```

Use powerful queries to directly retrieve scan results:

```sh
# Items whose path contains 'reports'
fspulse query "items where item_path:('reports')"

# Changes involving items detected as invalid
fspulse query "changes where val_new:(I) show default, val_old, val_new order by change_id desc"
```

---

## 🛠 Contributing

Contributions are welcomed! Please see our [Contribution Guide](https://gtunes-dev.github.io/fspulse/development.html) for instructions.

## License

FsPulse is released under the MIT License. See [LICENSE](LICENSE) for details.

