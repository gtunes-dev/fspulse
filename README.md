<img src="https://raw.githubusercontent.com/gtunes-dev/fspulse/main/assets/splash.png" alt="FsPulse logo" width="100%" style="max-width: 600px;">

# FsPulse

**FsPulse** is a Rust-based command-line tool that captures and analyzes the state of directories over time. It tracks changes, validates file integrity, and allows users to query results with a powerful, SQL-like syntax.

---

## 🚀 Key Capabilities

- **Directory Scanning** — Track file and folder additions, deletions, and modifications
- **Content Validation** — Validate file types such as FLAC, JPEG, PNG, GIF, BMP, and PDF
- **SHA2 Hashing** — Optionally detect content changes beyond metadata
- **Powerful Querying** — SQL-inspired query language for flexible results
- **Interactive Mode** — Menu-driven exploration once scanning is underway

---

## 📚 Documentation

Full user guide is available here:

👉 **[FsPulse Documentation](https://gtunes-dev.github.io/fspulse/)**

Key sections:
- [Getting Started](https://gtunes-dev.github.io/fspulse/getting_started.html)
- [Query Syntax](https://gtunes-dev.github.io/fspulse/query.html)
- [Command-Line Interface](https://gtunes-dev.github.io/fspulse/cli.html)
- [Interactive Mode](https://gtunes-dev.github.io/fspulse/interactive_mode.html)
- [Scanning](https://gtunes-dev.github.io/fspulse/scanning.html)
- [Validators](https://gtunes-dev.github.io/fspulse/validators.html)
- [Configuration](https://gtunes-dev.github.io/fspulse/configuration.html)

---

## 🛠 Building from Source

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
```

Run from the `target/release` directory:

```sh
./target/release/fspulse --help
```

---

## ⚡ Quick Examples

Run a scan:

```sh
fspulse scan --root-path /some/directory
```

Launch interactive mode:

```sh
fspulse interact
```

Use query syntax to explore results:

```sh
fspulse query "items where item_path:('reports')"

fspulse query "changes where val_new:(I) show default, val_old, val_new order by change_id desc"
```

---

## 🤝 Contributions

FsPulse is under active development, but is **not currently accepting external contributions**. This may change in the future — see our [Development Guide](https://gtunes-dev.github.io/fspulse/development.html) for details.

---

## 📄 License

Released under the MIT License. See [LICENSE](LICENSE) for details.

