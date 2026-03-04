# Trends

The Trends page provides interactive visualizations showing how your data evolves over time across multiple scans.

<!-- Screenshot: Trends page showing file size and change activity charts -->
<!-- ![Trends Charts](screenshot-placeholder-trends.png) -->

## Available Charts

### File Size Trends

Track total storage usage over time:
- See growth or reduction in directory sizes
- Identify storage bloat
- Displayed in both decimal (GB) and binary (GiB) units

### File/Folder Count Trends

Monitor the number of items:
- Total files and folders over time
- Detect unexpected additions or deletions
- Separate trend lines for files vs. directories

### Change Activity

Visualize filesystem activity:
- Additions, modifications, and deletions per scan
- Identify periods of high change
- Understand modification patterns

### Alert Trends

Track integrity issues over time:
- Validation failures
- Suspect hash changes
- Alert resolution patterns

## Features

### Root Selection

Select which scan root to analyze from the dropdown. Each root maintains independent trend data.

> **Tip**: If you select a root on the [Browse](browse.md) or [Alerts](alerts.md) page before navigating to Trends, the same root will be pre-selected automatically via the shared root context.

### Date Range Filtering

Customize the time window:
- Last 7 days
- Last 30 days
- Last 3 months
- Last 6 months
- Last year
- Custom range (manual date pickers)

### Baseline Exclusion

The Changes and Alerts charts offer a checkbox to exclude the first scan from the visualization. The first scan of a root often shows large numbers of "additions" and alerts which can skew trend visualizations. This toggle only appears when the first scan falls within the selected date range.

### Interactive Charts

- Hover for detailed values
- Pan and zoom on time ranges
- Toggle data series on/off

## Requirements

Trend analysis requires **multiple scans** of the same root. After your first scan, you'll see a message prompting you to run additional scans to generate trend data.

## Use Cases

- **Capacity Planning**: Monitor storage growth rates
- **Change Detection**: Identify unusual modification patterns
- **Validation Monitoring**: Track data integrity over time
- **Baseline Comparison**: See how your filesystem evolves from initial state
