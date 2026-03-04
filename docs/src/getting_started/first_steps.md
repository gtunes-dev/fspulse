# First Steps

This guide walks you through your first scan and basic usage of fsPulse.

## Starting the Web Interface

Launch fsPulse:

```sh
fspulse
```

Open your browser to **http://127.0.0.1:8080**

You'll see the **Dashboard** — fsPulse's home page. On first launch with no roots configured, it will display a welcome message prompting you to add your first root.

## Adding Your First Scan Root

1. Navigate to the **Setup** page (in the utility section of the sidebar)
2. On the **Roots** tab, click **Add Root**
3. Enter the path to the directory you want to monitor
4. Save

## Running Your First Scan

1. From the Setup page's Roots tab, click **Scan Now** for your newly added root
2. Watch the real-time progress on the **Dashboard** (the home page) — you'll see live statistics as fsPulse scans, sweeps for deleted items, and analyzes files
3. Once complete, explore the results

## Exploring Your Data

After your first scan completes:

- **Dashboard** — See the health status of your root, including any alerts generated
- **Browse** — Navigate your filesystem hierarchy with tree, folder, or search views. Open the detail panel to inspect any file's metadata, size history, version history, and alerts.
- **Alerts** — Check for any integrity issues detected (suspect hash changes, validation failures, access errors)
- **Trends** — View charts and trends (requires multiple scans to generate meaningful data)
- **Data Explorer** — Run queries against your scan data using the visual builder or free-form query language

## Setting Up Scheduled Scans

1. Navigate to the **Setup** page
2. Switch to the **Schedules** tab
3. Click **Add Schedule**
4. Select your root and configure:
   - Schedule type (daily, weekly, monthly, interval)
   - Time/day settings
   - Scan options (hashing, validation)
5. Save the schedule

Scheduled scans will run automatically based on your configuration. You can see upcoming tasks on the Dashboard and the full activity log on the History page.

## Next Steps

- Learn about [Scanning Concepts](../scanning.md) — how hashing and validation work
- Explore the [Interface](../web_ui.md) features in detail
- Understand the [Query Syntax](../query.md) for advanced analysis
