#!/bin/bash
set -e

# The FSPULSE_DATA_DIR environment variable is set in the Dockerfile
# This tells the app to use /data for config, database, and logs

# Get PUID/PGID from environment, default to 1000 if not set
PUID=${PUID:-1000}
PGID=${PGID:-1000}

# Display startup message
echo "FsPulse Docker Container"
echo "Data directory: ${FSPULSE_DATA_DIR}"
echo "Running as UID:GID ${PUID}:${PGID}"

# If PUID/PGID differ from default (1000), update the fspulse user
if [ "$PUID" != "1000" ] || [ "$PGID" != "1000" ]; then
    echo "Adjusting fspulse user to UID:GID ${PUID}:${PGID}..."

    # Modify group first
    groupmod -o -g "$PGID" fspulse 2>/dev/null || true

    # Modify user
    usermod -o -u "$PUID" fspulse 2>/dev/null || true
fi

# Always ensure /data directory has correct ownership
# (This is needed even with default UID/GID, in case the volume was created by root)
if [ -d "${FSPULSE_DATA_DIR}" ]; then
    echo "Ensuring ${FSPULSE_DATA_DIR} is owned by fspulse user..."
    chown -R fspulse:fspulse "${FSPULSE_DATA_DIR}" 2>/dev/null || \
        echo "Warning: Could not change ownership of ${FSPULSE_DATA_DIR} (may already be correct)"
fi

# Check if this is first run (no config file)
if [ ! -f "${FSPULSE_DATA_DIR}/config.toml" ]; then
    echo "First run detected - config.toml will be created with defaults"
fi

# Execute fspulse as the fspulse user (with the potentially adjusted UID/GID)
# The app will auto-create config.toml and database if they don't exist
#
# Note: gosu may fail on some platforms (e.g., Synology DSM). If it fails,
# we fall back to running as root, which is safe since FsPulse only reads files.
set +e  # Disable exit-on-error for gosu test
if gosu fspulse true 2>/dev/null; then
    # gosu works - use it to run as non-root user
    set -e  # Re-enable exit-on-error
    exec gosu fspulse /app/fspulse "$@"
else
    # gosu failed (common on Synology) - fall back to running as root
    echo ""
    echo "Warning: Could not run as user 'fspulse' (platform limitation)"
    echo "Running as root instead (safe for read-only operations)"
    echo ""
    set -e  # Re-enable exit-on-error
    exec /app/fspulse "$@"
fi
