# Docker Deployment

The easiest way to run FsPulse is with Docker. The container runs FsPulse as a background service with the web UI accessible on port 8080. You can manage roots, initiate scans, query data, and view results—all from your browser.

---

## Quick Start

Get FsPulse running in three simple steps:

```bash
# 1. Pull the image
docker pull gtunesdev/fspulse:latest

# 2. Run the container
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -v fspulse-data:/data \
  gtunesdev/fspulse:latest

# 3. Access the web UI
open http://localhost:8080
```

That's it! The web UI is now running.

This basic setup stores all FsPulse data (database, config, logs) in a Docker volume and uses default settings. **If you need to customize settings** (like running as a specific user for NAS deployments, or changing the port), see the [Configuration](#configuration) and [NAS Deployments](#nas-deployments-truenas-unraid) sections below.

---

## Scanning Your Files

To scan directories on your host machine, you need to mount them into the container. FsPulse can then scan these mounted paths.

### Mounting Directories

Add `-v` flags to mount host directories into the container. We recommend mounting them under `/roots` for clarity:

```bash
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -v fspulse-data:/data \
  -v ~/Documents:/roots/documents:ro \
  -v ~/Photos:/roots/photos:ro \
  gtunesdev/fspulse:latest
```

The `:ro` (read-only) flag is recommended for safety—FsPulse only reads files during scans and never modifies them.

### Creating Roots in the Web UI

After mounting directories:

1. Open http://localhost:8080 in your browser
2. Navigate to **Manage Roots** in the sidebar
3. Click **Add Root**
4. Enter the **container path**: `/roots/documents` (not the host path `~/Documents`)
5. Click **Create Root**

**Important**: Always use the container path (e.g., `/roots/documents`), not the host path. The container doesn't know about host paths.

Once roots are created, you can scan them from the web UI and monitor progress in real-time.

---

## Docker Compose (Recommended)

For persistent deployments, Docker Compose is cleaner and easier to manage:

```yaml
version: '3.8'

services:
  fspulse:
    image: gtunesdev/fspulse:latest
    container_name: fspulse
    restart: unless-stopped
    ports:
      - "8080:8080"
    volumes:
      # Persistent data storage - REQUIRED
      # Must map /data to either a Docker volume (shown here) or a host path
      # Must support read/write access for database, config, and logs
      - fspulse-data:/data

      # Alternative: use a host path instead
      # - /path/on/host/fspulse-data:/data

      # Directories to scan (read-only recommended for safety)
      - ~/Documents:/roots/documents:ro
      - ~/Photos:/roots/photos:ro
    environment:
      # Optional: override any configuration setting
      # See Configuration section below and https://gtunes-dev.github.io/fspulse/configuration.html
      - TZ=America/New_York

volumes:
  fspulse-data:
```

Save as `docker-compose.yml` and run:

```bash
docker-compose up -d
```

---

## Configuration

FsPulse creates a default `config.toml` on first run with sensible defaults. Most users won't need to change anything, but when you do, there are three ways to customize settings.

### Option 1: Use Environment Variables (Easiest)

Override any setting using environment variables. This works with both `docker run` and Docker Compose.

**Docker Compose example:**

```yaml
services:
  fspulse:
    image: gtunesdev/fspulse:latest
    environment:
      - FSPULSE_SERVER_PORT=9090      # Change web UI port
      - FSPULSE_LOGGING_FSPULSE=debug # Enable debug logging
      - FSPULSE_ANALYSIS_THREADS=16   # Use 16 analysis threads
    ports:
      - "9090:9090"
```

**Command line example (equivalent to above):**

```bash
docker run -d \
  --name fspulse \
  -p 9090:9090 \
  -e FSPULSE_SERVER_PORT=9090 \
  -e FSPULSE_LOGGING_FSPULSE=debug \
  -e FSPULSE_ANALYSIS_THREADS=16 \
  -v fspulse-data:/data \
  gtunesdev/fspulse:latest
```

Environment variables follow the pattern `FSPULSE_<SECTION>_<FIELD>` and override any settings in `config.toml`. See the [Configuration](configuration.md#environment-variables) chapter for a complete list of available variables and their purposes.

### Option 2: Edit the Config File

If you prefer editing the config file directly:

1. Extract the auto-generated config:
   ```bash
   docker exec fspulse cat /data/config.toml > config.toml
   ```

2. Edit `config.toml` with your preferred settings

3. Copy back and restart:
   ```bash
   docker cp config.toml fspulse:/data/config.toml
   docker restart fspulse
   ```

### Option 3: Pre-Mount Your Own Config (Advanced)

If you want custom settings before first launch, create your own `config.toml` and mount it:

```yaml
volumes:
  - fspulse-data:/data
  - ./my-config.toml:/data/config.toml:ro
```

Most users should start with Option 1 (environment variables) or Option 2 (edit after first run).

---

## NAS Deployments (TrueNAS, Unraid)

NAS systems often have specific user IDs for file ownership. By default, FsPulse runs as user 1000, but you may need it to match your file ownership.

### Setting User and Group IDs

Use `PUID` and `PGID` environment variables to run FsPulse as a specific user:

**TrueNAS Example** (apps user = UID 34):
```bash
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -e PUID=34 \
  -e PGID=34 \
  -e TZ=America/New_York \
  -v /mnt/pool/fspulse/data:/data \
  -v /mnt/pool/documents:/roots/docs:ro \
  gtunesdev/fspulse:latest
```

**Unraid Example** (custom UID 1001):
```bash
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -e PUID=1001 \
  -e PGID=100 \
  -v /mnt/user/appdata/fspulse:/data \
  -v /mnt/user/photos:/roots/photos:ro \
  gtunesdev/fspulse:latest
```

### Why PUID/PGID Matters

Even though you mount directories as read-only (`:ro`), Linux permissions still apply. If your files are owned by UID 34 and aren't world-readable, FsPulse (running as UID 1000 by default) won't be able to scan them. Setting `PUID=34` makes FsPulse run as the same user that owns the files.

**When to use PUID/PGID:**
- Files have restrictive permissions (not world-readable)
- Using NAS with specific user accounts (TrueNAS, Unraid, Synology)
- You need the `/data` directory to match specific host ownership

---

## Advanced Topics

### Custom Network Settings

If you're using macvlan or host networking, ensure the server binds to all interfaces:

```yaml
services:
  fspulse:
    image: gtunesdev/fspulse:latest
    network_mode: host
    environment:
      - FSPULSE_SERVER_HOST=0.0.0.0  # Required for non-bridge networking
      - FSPULSE_SERVER_PORT=8080
```

**Note**: The Docker image sets `FSPULSE_SERVER_HOST=0.0.0.0` by default, so this is only needed if your config.toml overrides it to `127.0.0.1`.

### Reverse Proxy Setup

For public access with authentication, use a reverse proxy like nginx:

```nginx
server {
    listen 80;
    server_name fspulse.example.com;

    location / {
        proxy_pass http://localhost:8080;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;

        # WebSocket support for scan progress
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

### Using Bind Mounts Instead of Volumes

By default, we use Docker volumes (`-v fspulse-data:/data`) which Docker manages automatically. For NAS deployments, you might prefer bind mounts to integrate with your existing backup schemes:

```bash
# Create directory on host
mkdir -p /mnt/pool/fspulse/data

# Use bind mount
docker run -d \
  --name fspulse \
  -p 8080:8080 \
  -v /mnt/pool/fspulse/data:/data \  # Bind mount to host path
  gtunesdev/fspulse:latest
```

**Benefits of bind mounts for NAS:**
- Included in your NAS snapshot schedule
- Backed up with your existing backup system
- Directly accessible for manual inspection

**Trade-off**: You need to manage permissions yourself (use PUID/PGID if needed).

---

## Troubleshooting

### Cannot Access Web UI

**Problem**: http://localhost:8080 doesn't respond

**Solutions:**

1. Check the container is running:
   ```bash
   docker ps | grep fspulse
   ```

2. Check logs for errors:
   ```bash
   docker logs fspulse
   ```
   Look for "Server started" message.

3. Verify port mapping:
   ```bash
   docker port fspulse
   ```
   Should show `8080/tcp -> 0.0.0.0:8080`

### Permission Denied Errors

**Problem**: "Permission denied" when scanning or accessing `/data`

**Solutions:**

1. Check file ownership:
   ```bash
   ls -ln /path/to/your/files
   ```

2. Set PUID/PGID to match file owner:
   ```bash
   docker run -e PUID=1000 -e PGID=1000 ...
   ```

3. For bind mounts, ensure host directory is writable:
   ```bash
   chown -R 1000:1000 /mnt/pool/fspulse/data
   ```

### Configuration Changes Don't Persist

**Problem**: Settings revert after container restart

**Solution**: Verify `/data` volume is mounted:
```bash
docker inspect fspulse | grep -A 10 Mounts
```

If missing, recreate container with volume:
```bash
docker stop fspulse
docker rm fspulse
docker run -d --name fspulse -v fspulse-data:/data ...
```

### Database Locked Errors

**Problem**: "Database is locked" errors

**Cause**: Multiple containers accessing the same database

**Solution**: Only run one FsPulse container per database. Don't mount the same `/data` volume to multiple containers.

---

## Data Backup

### Backing Up Your Data

**For Docker volumes:**
```bash
# Stop container
docker stop fspulse

# Backup volume
docker run --rm \
  -v fspulse-data:/data \
  -v $(pwd):/backup \
  alpine tar czf /backup/fspulse-backup.tar.gz /data

# Restart container
docker start fspulse
```

**For bind mounts:**
```bash
# Simply backup the host directory
tar czf fspulse-backup.tar.gz /mnt/pool/fspulse/data
```

### Restoring from Backup

**For Docker volumes:**
```bash
# Create volume
docker volume create fspulse-data-restored

# Restore data
docker run --rm \
  -v fspulse-data-restored:/data \
  -v $(pwd):/backup \
  alpine sh -c "cd / && tar xzf /backup/fspulse-backup.tar.gz"

# Use restored volume
docker run -d --name fspulse -v fspulse-data-restored:/data ...
```

**For bind mounts:**
```bash
tar xzf fspulse-backup.tar.gz -C /mnt/pool/fspulse/data
```

---

## Image Tags and Updates

FsPulse provides multiple tags for different update strategies:

| Tag | Description | When to Use |
|-----|-------------|-------------|
| `latest` | Latest stable release | Production (pinned versions) |
| `1.2.3` | Specific version | Production (exact control) |
| `1.2` | Latest patch of minor version | Production (auto-patch updates) |
| `main` | Development builds | Testing new features |

**Recommendation**: Use specific version tags (`1.2.3`) or minor version tags (`1.2`) for production. Avoid `latest` in production to prevent unexpected updates.

**Updating to a new version:**
```bash
docker pull gtunesdev/fspulse:1.2.3
docker stop fspulse
docker rm fspulse
docker run -d --name fspulse -v fspulse-data:/data ... gtunesdev/fspulse:1.2.3
```

Your data persists in the volume across updates.

---

## Platform Support

FsPulse images support multiple architectures—Docker automatically pulls the correct one for your platform:

- **linux/amd64** - Intel/AMD processors (most common)
- **linux/arm64** - ARM processors (Apple Silicon, Raspberry Pi 4, ARM servers)

---

## Next Steps

- Explore the [Configuration](configuration.md) reference for all available settings
- Learn about [Query Syntax](query.md) for advanced data filtering
- Read [Scanning](scanning.md) to understand how scans work

---

## Getting Help

- **Issues**: [GitHub Issues](https://github.com/gtunes-dev/fspulse/issues)
- **Docker Hub**: [gtunesdev/fspulse](https://hub.docker.com/r/gtunesdev/fspulse)
- **Documentation**: [FsPulse Book](https://gtunes-dev.github.io/fspulse/)
