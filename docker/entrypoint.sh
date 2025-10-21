#!/bin/bash
set -e

# The FSPULSE_DATA_DIR environment variable is set in the Dockerfile
# This tells the app to use /data for config, database, and logs

# Display startup message
echo "FsPulse Docker Container"
echo "Data directory: ${FSPULSE_DATA_DIR}"

# Check if this is first run (no config file)
if [ ! -f "${FSPULSE_DATA_DIR}/config.toml" ]; then
    echo "First run detected - config.toml will be created with defaults"
fi

# Execute fspulse with provided command
# The app will auto-create config.toml and database if they don't exist
exec /app/fspulse "$@"
