import { formatMoment } from '@/lib/timeRange'

interface TrackingLabelProps {
  /** When tracking stopped (Unix seconds); null if the item is tracked. */
  ignoredSince?: number | null
  /** The item was ignored at the start of the range and is tracked again. */
  resumed?: boolean
}

/**
 * Says, in words, that an item is not tracked because it matches one of
 * its root's ignore patterns, or that tracking of it has resumed.
 * Ignoring is not deleting, so it gets its own label instead of the
 * deleted marker.
 */
export function TrackingLabel({ ignoredSince, resumed }: TrackingLabelProps) {
  if (ignoredSince != null) {
    return (
      <span
        className="shrink-0 whitespace-nowrap rounded border border-border px-1.5 text-xs text-muted-foreground"
        title="Matches one of this root's ignore patterns. Nothing has been recorded about it since then; its earlier history is kept."
      >
        Ignored since {formatMoment(ignoredSince)}
      </span>
    )
  }
  if (resumed) {
    return (
      <span
        className="shrink-0 whitespace-nowrap rounded border border-border px-1.5 text-xs text-muted-foreground"
        title="Was ignored at the start of this range and is tracked again."
      >
        Tracking resumed
      </span>
    )
  }
  return null
}
