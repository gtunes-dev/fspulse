#!/bin/bash

# fsPulse Build Script
# --------------------
# Builds both the frontend and backend for fsPulse.
# This script handles the full build process including:
# - Installing frontend dependencies
# - Building the React frontend
# - Building the Rust binary with embedded assets

set -euo pipefail

echo "🏗️  fsPulse Build Script"
echo "======================="
echo

# Check for required tools
echo "🔍 Checking prerequisites..."

if ! command -v node &> /dev/null; then
    echo "❌ ERROR: Node.js is not installed"
    echo "   Install Node.js from: https://nodejs.org/"
    exit 1
fi

if ! command -v npm &> /dev/null; then
    echo "❌ ERROR: npm is not installed"
    echo "   npm usually comes with Node.js"
    exit 1
fi

if ! command -v cargo &> /dev/null; then
    echo "❌ ERROR: Cargo is not installed"
    echo "   Install Rust from: https://rustup.rs/"
    exit 1
fi

echo "✅ All prerequisites found"
echo

# Build frontend
echo "📦 Building frontend..."
echo "   Location: frontend/"
cd frontend

if [ ! -d "node_modules" ]; then
    echo "   Installing dependencies..."
    npm install
fi

echo "   Running build..."
npm run build

cd ..
echo "✅ Frontend build complete"
echo

# Build Rust binary
echo "🦀 Building Rust binary..."
cargo build --release

echo
echo "✅ Build complete!"
echo
echo "The binary is located at: ./target/release/fspulse"
echo
echo "To run fsPulse:"
echo "  ./target/release/fspulse serve"
echo
