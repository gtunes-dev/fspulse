# First Steps

This guide walks you through your first scan and basic usage of FsPulse.

## Starting the Web Interface

Launch FsPulse:

```sh
fspulse
```

Open your browser to **http://127.0.0.1:8080**

## Adding Your First Scan Root

1. Navigate to the **Monitor** page
2. Click **Add Root** 
3. Enter the path to the directory you want to monitor
4. Configure initial scan settings

## Running Your First Scan

1. From the Monitor page, click **Scan Now** for your newly added root
2. Watch the real-time progress on the **Tasks** page (the home page)
3. Once complete, explore the results

## Exploring Your Data

After your first scan completes:

- **Browse** — Navigate your filesystem hierarchy
- **Insights** — View charts and trends (requires multiple scans)
- **Alerts** — Check for any validation issues detected
- **Explore** — Run queries against your scan data

## Setting Up Scheduled Scans

1. Navigate to the **Monitor** page
2. Click **Add Schedule**
3. Select your root and configure:
   - Schedule type (daily, weekly, monthly, interval)
   - Time/day settings
   - Scan options (hashing, validation)
4. Save the schedule

Scheduled scans will run automatically based on your configuration.

## Next Steps

- Learn about [Scanning Concepts](../scanning.md)
- Explore the [Interface](../web_ui.md) features
- Understand the [Query Syntax](../query.md) for advanced analysis
