# Integrity

The Integrity page provides a centralized view for reviewing and managing integrity issues detected during scans. It surfaces two kinds of issues: **suspect hashes** (file content changed without a metadata change) and **validation errors** (format validation detected corruption).

## Issue Types

### Suspicious Hashes

Detected when:
- A file's hash changes between scans
- The file's modification time and size have NOT changed

This pattern indicates potential:
- Bit rot (silent data corruption)
- Tampering or malicious modification
- Filesystem anomalies

### Validation Errors

Detected when format validation fails:
- FLAC audio files with invalid structure
- JPEG/PNG images that fail format checks
- PDF files with corruption
- Other validated file types with detected issues

See [Validators](../validators.md) for details on supported file types.

## Review Status

Each integrity issue can be in one of two states:

- **Unreviewed**: The issue has not been acknowledged by the user
- **Reviewed**: The user has acknowledged the issue

Marking an issue as reviewed records a timestamp. Review status is tracked independently for hash issues and validation issues on each item version.

## Filtering

Filter integrity issues by:
- **Issue type** — Suspicious hashes, Validation errors, or All
- **File type** — All file types, Image files, PDF files, Audio files
- **Review status** — Not Reviewed, Reviewed, or All
- **Root** — Show issues for a specific monitored directory
- **Path search** — Filter by item path
- **Show deleted** — Include or exclude items that are currently deleted

> **Tip**: If you select a root on the [Browse](browse.md) or [Trends](trends.md) page before navigating to Integrity, the same root will be pre-selected automatically via the shared root context.

## Issue Table

The main table shows one row per item that has integrity issues. Each row displays:

- **Validate toggle** — Enable or disable future validation for this item
- **File name** — With parent folder context
- **Hashes** — Count of unreviewed and reviewed hash issues
- **Validation** — Count of unreviewed and reviewed validation issues
- **Review All** — Button to mark all issues on this item as reviewed

### Expanding Items

Click the expand toggle on any row to see the version history for that item, showing detailed hash and validation state for each version. From the expanded view you can review individual issues at the version level.

## Reviewing Issues

Reviews are a lightweight acknowledgment mechanism — they indicate that you have seen and considered an integrity issue.

- **Review individual issues**: Expand an item and use the review toggle on a specific version's hash or validation issue
- **Review all issues on an item**: Click the "Review All" button on the item row
- Review status can be toggled back to unreviewed if needed

## Integration with Browse

Integrity issues are also visible in the [Browse](browse.md) page's item detail panel, where you can see hash and validation state for each version and toggle review status directly.

## Workflow Recommendations

1. **Check Home**: The [Home](home.md) page shows integrity issue counts per root in the recent activity section
2. **Filter**: Use issue type and review status filters to focus on what matters
3. **Investigate**: Expand items to see version details, or click through to Browse for full context
4. **Review**: Mark issues as reviewed once you've assessed them
5. **Track**: Monitor integrity trends on the [Trends](trends.md) page
