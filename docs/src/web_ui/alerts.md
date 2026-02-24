# Alerts

The Alerts page provides a centralized view for managing integrity issues detected during scans.

## Alert Types

FsPulse generates three types of alerts:

### Access Denied

Triggered when FsPulse is unable to access an item or folder. These alerts can occur during either the scan phase or the analysis phase:

**During Scan Phase:**
- Unable to retrieve item metadata (type, size, or modification date)
- Unable to enumerate folder contents (typically due to permission restrictions)

**During Analysis Phase:**
- Unable to read a file for hashing or validation

**Notes:**
- If FsPulse cannot determine an item's type from metadata, the item is recorded as an instance of the "Unknown" type
- Items with failed metadata retrieval, whether "Unknown" or otherwise, are not examined during the analysis phase

### Suspicious Hash Changes

Triggered when:
- A file's hash changes between scans
- The file's modification time has NOT changed

This pattern indicates potential:
- Bit rot (silent data corruption)
- Tampering or malicious modification
- Filesystem anomalies

### Invalid Items

Triggered when format validation fails:
- FLAC audio files with invalid structure
- JPEG/PNG images that fail format checks
- PDF files with corruption
- Other validated file types with detected issues

See [Validators](../validators.md) for details on supported file types.

## Alert Status

Each alert can be in one of three states:

- **Open**: New alert requiring attention
- **Flagged**: Marked for follow-up or further investigation
- **Dismissed**: Reviewed and determined to be non-critical

## Managing Alerts

### Filtering

Filter alerts by:
- Status (Open/Flagged/Dismissed)
- Alert type (Access denied/Hash change/Validation failure)
- Root
- Time range
- Path search

### Status Actions

- **Flag**: Mark alert for follow-up
- **Dismiss**: Acknowledge and close the alert
- **Reopen**: Change dismissed alert back to open

### Batch Operations

Select multiple alerts to update status in bulk.

## Alert Details

Click an alert to view:
- Item path and metadata
- Alert timestamp
- Access error details (for access denied alerts)
- Hash change details: previous and new hash values (for suspicious hash alerts)
- Validation error message (for invalid items)
- Link to item in Browse view

## Integration with Browse

Alerts are also displayed in the [Browse](browse.md) page's item detail panel, providing context when investigating specific files.

## Workflow Recommendations

1. **Review Open Alerts**: Check new alerts regularly
2. **Investigate**: Use Browse to examine affected items
3. **Triage**: Flag important issues, dismiss false positives
4. **Restore**: Use backups to restore corrupted files if needed
5. **Track**: Monitor alert trends in [Insights](insights.md)
