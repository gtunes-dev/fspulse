# Installation

FsPulse can be installed in several ways depending on your preferences and environment.

## Docker Hub (Recommended)

Pull the official image and run:

```sh
docker pull gtunesdev/fspulse:latest
docker run -d --name fspulse -p 8080:8080 -v fspulse-data:/data gtunesdev/fspulse:latest
```

Multi-architecture support: `linux/amd64`, `linux/arm64`

See [Docker Deployment](../docker.md) for complete instructions.

## Cargo (crates.io)

Install via Rust's package manager:

```sh
cargo install fspulse
```

Requires Rust toolchain installed on your system.

## Pre-built Binaries

Download platform-specific binaries from [GitHub Releases](https://github.com/gtunes-dev/fspulse/releases).

Available for: Linux, macOS, Windows

macOS builds include both Intel (x86_64) and Apple Silicon (ARM64) binaries.

**Note:** All web UI assets are embedded in the binary—no external files or dependencies required.

## Build from Source

Clone and build with Cargo:

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
cargo build --release
./target/release/fspulse --help
```

## Next Steps

After installation, proceed to [First Steps](first_steps.md) to configure and start using FsPulse.
