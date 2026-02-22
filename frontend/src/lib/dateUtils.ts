/**
 * Date formatting utilities for FsPulse frontend
 * All functions accept Unix timestamps in SECONDS (as returned by backend with @timestamp)
 * All formatting is done in the user's local timezone
 */

/**
 * Format timestamp as full date/time string: YYYY-MM-DD HH:MM:SS
 * Matches the server's full format but in user's local timezone
 * Used for: Explore page table displays
 */
export function formatDateFull(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000) // Convert seconds to milliseconds

  const year = date.getFullYear()
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const hours = String(date.getHours()).padStart(2, '0')
  const minutes = String(date.getMinutes()).padStart(2, '0')
  const seconds = String(date.getSeconds()).padStart(2, '0')

  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`
}

/**
 * Format timestamp as relative time for recent dates, absolute for older
 * Used for: Scan page roots table, schedule next scan time
 *
 * Returns:
 * - "Today" / "Yesterday" / "Nd ago" for last 7 days (past)
 * - "Tomorrow" / "In Nd" for next 7 days (future)
 * - "Mon DD, YYYY" for older/further dates
 */
export function formatDateRelative(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000)
  const now = new Date()

  // Calculate difference in calendar days (not just 24-hour periods)
  // Normalize both dates to midnight to compare calendar days
  const dateOnly = new Date(date.getFullYear(), date.getMonth(), date.getDate())
  const nowOnly = new Date(now.getFullYear(), now.getMonth(), now.getDate())
  const daysDiff = Math.round((nowOnly.getTime() - dateOnly.getTime()) / (1000 * 60 * 60 * 24))

  // Handle future dates (negative daysDiff)
  if (daysDiff < 0) {
    const futureDays = Math.abs(daysDiff)
    if (futureDays === 0) return 'Today'
    if (futureDays === 1) return 'Tomorrow'
    if (futureDays < 7) return `In ${futureDays}d`
    // For future dates >= 7 days, fall through to formatted date
  } else {
    // Handle past dates (positive daysDiff)
    if (daysDiff === 0) return 'Today'
    if (daysDiff === 1) return 'Yesterday'
    if (daysDiff < 7) return `${daysDiff}d ago`
  }

  // Otherwise show formatted date
  return date.toLocaleDateString(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric'
  })
}

/**
 * Format timestamp as compact relative time
 * Used for: Alerts page "Created" column
 *
 * Returns:
 * - "Just now" for <1 minute
 * - "Nm ago" for <1 hour
 * - "Nh ago" for <1 day
 * - "Nd ago" for <1 month
 * - "Nmo ago" for <1 year
 * - "Ny ago" for >=1 year
 */
export function formatTimeAgo(timestampSeconds: number): string {
  const now = Date.now()
  const timestampMs = timestampSeconds * 1000
  const seconds = Math.floor((now - timestampMs) / 1000)

  if (seconds < 60) return 'Just now'
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`
  if (seconds < 2592000) return `${Math.floor(seconds / 86400)}d ago`
  if (seconds < 31536000) return `${Math.floor(seconds / 2592000)}mo ago`
  return `${Math.floor(seconds / 31536000)}y ago`
}

/**
 * Format timestamp as short date only: MM/DD/YYYY
 * Used for: ItemDetailSheet scan dates
 */
export function formatDateShort(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000)
  const month = String(date.getMonth() + 1).padStart(2, '0')
  const day = String(date.getDate()).padStart(2, '0')
  const year = date.getFullYear()
  return `${month}/${day}/${year}`
}

/**
 * Format timestamp as compact date: "d MMM yyyy" (e.g. "1 Dec 2024")
 * Used for: Scan references throughout the UI
 */
export function formatScanDate(timestampSeconds: number): string {
  const date = new Date(timestampSeconds * 1000)
  return date.toLocaleDateString('en-GB', {
    day: 'numeric',
    month: 'short',
    year: 'numeric',
  })
}
