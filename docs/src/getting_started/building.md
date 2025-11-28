# Building from Source

This guide covers building FsPulse from source code, which is useful for development, customization, or running on platforms without pre-built binaries.

## Prerequisites

Before building FsPulse, ensure you have the following installed:

### Required Tools

1. **Rust** (latest stable version)
   - Install via [rustup](https://rustup.rs/)
   - Verify: `cargo --version`

2. **Node.js** (v18 or later) with npm
   - Install from [nodejs.org](https://nodejs.org/)
   - Verify: `node --version` and `npm --version`

### Platform-Specific Requirements

**Windows:**
- Visual Studio Build Tools or Visual Studio with C++ development tools
- Required for SQLite compilation

**Linux:**
- Build essentials: `build-essential` (Ubuntu/Debian) or `base-devel` (Arch)
- May need `pkg-config` and `libsqlite3-dev` depending on distribution

**macOS:**
- Xcode Command Line Tools: `xcode-select --install`

## Quick Build (Recommended)

The easiest way to build FsPulse is using the provided build script:

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
./scripts/build.sh
```

The script will:
1. Check for required tools
2. Install frontend dependencies
3. Build the React frontend
4. Compile the Rust binary with embedded assets

The resulting binary will be at: `./target/release/fspulse`

## Manual Build

If you prefer to run each step manually or need more control:

### Step 1: Clone the Repository

```sh
git clone https://github.com/gtunes-dev/fspulse.git
cd fspulse
```

### Step 2: Build the Frontend

```sh
cd frontend
npm install
npm run build
cd ..
```

This creates the `frontend/dist/` directory containing the compiled React application.

### Step 3: Build the Rust Binary

```sh
cargo build --release
```

The binary will be at: `./target/release/fspulse`

> **Important:** The frontend **must** be built before compiling the Rust binary. The web UI assets are embedded into the binary at compile time via RustEmbed. If `frontend/dist/` doesn't exist, the build will fail with a helpful error message.

## Development Builds

For development, you can skip the release optimization:

```sh
# Frontend (development mode with hot reload)
cd frontend
npm run dev

# Backend (in a separate terminal)
cargo run -- serve
```

In development mode, the backend serves frontend files directly from `frontend/dist/` rather than using embedded assets, allowing for faster iteration.

## Troubleshooting

### "Frontend assets not found"

**Error:**
```text
❌ ERROR: Frontend assets not found at 'frontend/dist/'
```

**Solution:** Build the frontend first:
```sh
cd frontend
npm install
npm run build
cd ..
```

### Windows: "link.exe not found"

**Error:** Missing Visual Studio Build Tools

**Solution:** Install Visual Studio Build Tools with C++ development tools from [visualstudio.microsoft.com](https://visualstudio.microsoft.com/downloads/)

### Linux: "cannot find -lsqlite3"

**Error:** Missing SQLite development libraries

**Solution:** Install platform-specific package:
- Ubuntu/Debian: `sudo apt-get install libsqlite3-dev`
- Fedora: `sudo dnf install sqlite-devel`
- Arch: `sudo pacman -S sqlite`

### npm install fails

**Error:** Network or permission issues with npm

**Solution:**
- Clear npm cache: `npm cache clean --force`
- Check Node.js version: `node --version` (should be v18+)
- Try with sudo (not recommended) or fix npm permissions

## Running Your Build

After building, run FsPulse:

```sh
./target/release/fspulse --help
./target/release/fspulse serve
```

Access the web UI at: **http://localhost:8080**

## Next Steps

- [First Steps](first_steps.md) - Configure and start using FsPulse
- [Configuration](../configuration.md) - Customize FsPulse behavior
- [Development](../development.md) - Contributing to FsPulse
