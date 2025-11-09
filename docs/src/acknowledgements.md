# Acknowledgements

FsPulse relies on several open source Rust crates. We gratefully acknowledge the work of these maintainers, particularly for enabling file format validation.

## File Format Validation

The following libraries enable FsPulse's ability to detect corrupted files:

- [`claxon`](https://github.com/ruuda/claxon) — FLAC audio decoding and validation
- [`image`](https://github.com/image-rs/image) — Image format decoding for JPG, PNG, GIF, TIFF, BMP
- [`lopdf`](https://github.com/J-F-Liu/lopdf) — PDF parsing and validation

See [Validators](validators.md) for the complete list of supported file types.

## Additional Dependencies

FsPulse wouldn't be possible without the incredible open source ecosystem it's built upon:

**Web Interface:**
- [shadcn/ui](https://ui.shadcn.com) — Beautiful, accessible component library
- [Radix UI](https://www.radix-ui.com) — Unstyled, accessible UI primitives
- [Tailwind CSS](https://tailwindcss.com) — Utility-first CSS framework
- [Lucide](https://lucide.dev) — Clean, consistent icon set
- [React](https://react.dev) — UI framework

**Backend & CLI:**
- [rusqlite](https://github.com/rusqlite/rusqlite) — SQLite database interface
- [axum](https://github.com/tokio-rs/axum) — Web framework
- [tokio](https://tokio.rs) — Async runtime
- [clap](https://github.com/clap-rs/clap) — Command-line argument parsing
- [dialoguer](https://github.com/console-rs/dialoguer) — Interactive CLI prompts
- [ratatui](https://ratatui.rs) — Terminal UI framework

The complete list of dependencies is available in the project's [`Cargo.toml`](https://github.com/gtunes-dev/fspulse/blob/main/Cargo.toml) and [`package.json`](https://github.com/gtunes-dev/fspulse/blob/main/web/package.json).

---

Thank you to all the open source maintainers whose work makes FsPulse possible.
