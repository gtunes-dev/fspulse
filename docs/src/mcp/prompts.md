# Sample Prompts

Once connected, try prompts like these. The agent will choose which tools to call based on your request.

## Getting Started

> Give me an overview of what fsPulse is monitoring

> Are there any integrity issues? Show me the details

## File Analysis

> What are the largest files being tracked?

> Break down the files by extension with counts and total sizes

> How many files of each type are there? Which types take the most space?

## Integrity Investigation

> Show me all suspect hash observations

> Show me validation failures for PDF files

## Scan History

> Show me recent scan activity for /home/user/Documents

> What changed in the last scan?

## Browsing and Searching

> List the contents of /Users/greg/Documents

> Search for files named "report"

## Aggregation

> Count files by extension for root 3, sorted by total size

> How many scans has each root had?

## Multi-Step Investigations

The most powerful use of fsPulse with an AI agent is iterative investigation — start with a high-level question and drill down based on what you find. These are examples of conversations, not single prompts.

### Activity Report for a Time Period

Start broad, then focus:

> What changed in root 1 between March 1 and March 15?

From the results, you might follow up with:

> Drill into the /photos/2026 folder — what was added there?

> Which files were modified more than once during that period?

### Storage Growth Analysis

> Show me how the total size of root 2 has changed over the last 20 scans

> Graph the file count and total size trends

> Which folders are growing the fastest? Break down size by top-level directory

### Investigating Churn

> Which files in root 1 have been modified the most times?

> Show me the version history for that file — how has its size changed over time?

> Graph the size of that file over its version history

### Integrity Triage

> Show me all unreviewed integrity issues for root 3

> Focus on the PDF validation failures — are they concentrated in a specific folder?

> Show me the version history for the files with suspect hashes — when did the hash first change?
