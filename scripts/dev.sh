#!/bin/bash
# Development script for running the full stack
# Kills existing backend, builds, ensures Vite is running, then starts backend

set -e

# Kill existing backend if running
echo "Stopping any existing backend..."
pkill -f 'target/debug/fspulse' 2>/dev/null || true

# Build backend
echo "Building backend..."
cargo build

# Ensure vite is running
if ! pgrep -f 'vite.*frontend' > /dev/null; then
    echo "Starting Vite dev server..."
    (cd frontend && npm run dev &)
    sleep 2  # Give vite time to start
else
    echo "Vite dev server already running"
fi

# Run backend
echo "Starting backend..."
./target/debug/fspulse serve
