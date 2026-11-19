import { useState, useEffect, useMemo, useRef, useCallback } from 'react'
import { format, startOfDay, startOfHour } from 'date-fns'
import { ChevronDown, Plus, RefreshCw, Triangle, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { cn } from '@/lib/utils'
import { formatCount } from '@/lib/formatUtils'
import {
  DAY,
  HOUR,
  PRESETS,
  describeRange,
  isRange,
  nowSeconds,
  presetSelection,
  rangeKey,
  rangeParams,
  type PresetId,
  type ResolvedRange,
  type TimeRangeSelection,
} from '@/lib/timeRange'

interface TimeRangeBarProps {
  rootId: number
  selection: TimeRangeSelection
  onSelectionChange: (selection: TimeRangeSelection) => void
  /** The preset the initial selection came from, if any. */
  initialPreset?: PresetId
  /** Changes whenever the data should be read again. */
  refreshToken: number
  onRefresh: () => void
  /** Turn on the listing of ignored items. When given, the "N ignored" count is a button for it. */
  onShowIgnored?: () => void
}

interface ActivityPoint {
  time: number
  added: number
  modified: number
  deleted: number
}

interface ChangeCounts {
  added: number
  modified: number
  deleted: number
  transient: number
  /** Stopped being tracked because of an ignore pattern. Not deletions. */
  ignored: number
  /** Tracked again after being ignored. Not additions. */
  resumed: number
}

/** Which stretch of time the histogram covers, and so how wide a bar is. */
type Granularity = 'all' | 'day' | 'hour'

interface Bucket {
  /** Start, inclusive, and end, exclusive, in Unix seconds. */
  start: number
  end: number
  added: number
  modified: number
  deleted: number
}

const DAY_BUCKETS = 60
const HOUR_BUCKETS = 48
/** Bars in the all-history view; each covers a whole number of days. */
const ALL_BUCKETS = 60

/**
 * Continuous buckets ending at the present, with each active hour's
 * counts added to the bucket it falls in. Day buckets follow the
 * viewer's local midnight.
 */
function buildBuckets(points: ActivityPoint[], granularity: Granularity, now: number): Bucket[] {
  const buckets: Bucket[] = []
  if (granularity === 'day' || granularity === 'all') {
    // Whole days per bar: one for the day view; for all history, as
    // many as it takes to reach back to the first recorded change.
    let daysPerBucket = 1
    let count = DAY_BUCKETS
    if (granularity === 'all') {
      const first = points.length > 0 ? points[0].time : now
      const daysOfHistory = Math.max(1, Math.ceil((now - first) / DAY) + 1)
      daysPerBucket = Math.max(1, Math.ceil(daysOfHistory / ALL_BUCKETS))
      count = Math.min(ALL_BUCKETS, Math.ceil(daysOfHistory / daysPerBucket))
    }
    const cursor = startOfDay(new Date(now * 1000))
    cursor.setDate(cursor.getDate() - (count * daysPerBucket - 1))
    for (let i = 0; i < count; i++) {
      const start = Math.floor(cursor.getTime() / 1000)
      cursor.setDate(cursor.getDate() + daysPerBucket)
      buckets.push({ start, end: Math.floor(cursor.getTime() / 1000), added: 0, modified: 0, deleted: 0 })
    }
  } else {
    const first = Math.floor(startOfHour(new Date(now * 1000)).getTime() / 1000) - (HOUR_BUCKETS - 1) * HOUR
    for (let i = 0; i < HOUR_BUCKETS; i++) {
      const start = first + i * HOUR
      buckets.push({ start, end: start + HOUR, added: 0, modified: 0, deleted: 0 })
    }
  }

  for (const point of points) {
    const bucket = buckets.find((b) => point.time >= b.start && point.time < b.end)
    if (bucket) {
      bucket.added += point.added
      bucket.modified += point.modified
      bucket.deleted += point.deleted
    }
  }
  return buckets
}

function bucketTotal(bucket: Bucket): number {
  return bucket.added + bucket.modified + bucket.deleted
}

function bucketLabel(bucket: Bucket, granularity: Granularity): string {
  const start = new Date(bucket.start * 1000)
  const lastDay = new Date((bucket.end - 1) * 1000)
  const when =
    granularity === 'hour'
      ? format(start, 'EEE MMM d, HH:00')
      : bucket.end - bucket.start > DAY + HOUR
        ? `${format(start, 'MMM d yyyy')} to ${format(lastDay, 'MMM d yyyy')}`
        : format(start, 'EEE MMM d yyyy')
  const total = bucketTotal(bucket)
  if (total === 0) return `${when}: no changes`
  return `${when}: ${formatCount(bucket.added)} added, ${formatCount(bucket.modified)} modified, ${formatCount(bucket.deleted)} deleted`
}

/** Mirror of the server's rule, for a point in time, where no request is needed. */
function resolvePoint(selection: TimeRangeSelection, now: number): ResolvedRange {
  if (selection.to === null || selection.to >= now) {
    return { from: null, to: now, to_is_now: true }
  }
  return { from: null, to: selection.to - (selection.to % HOUR), to_is_now: false }
}

/**
 * The time control for a root: says what time or range is being shown,
 * and lets the user change it with presets or by dragging across a
 * histogram of the root's activity.
 *
 * The histogram is the picker: the user chooses a range by seeing where
 * the changes are, rather than recalling dates.
 */
export function TimeRangeBar({ rootId, selection, onSelectionChange, initialPreset, refreshToken, onRefresh, onShowIgnored }: TimeRangeBarProps) {
  const [open, setOpen] = useState(false)
  const [presetId, setPresetId] = useState<PresetId | 'custom'>(initialPreset ?? 'custom')
  const [granularity, setGranularity] = useState<Granularity>('day')
  // Until the user picks a view, show the one that has something in it.
  const granularityChosen = useRef(false)

  const [points, setPoints] = useState<ActivityPoint[]>([])
  const [activityError, setActivityError] = useState<string | null>(null)

  const [resolved, setResolved] = useState<ResolvedRange | null>(null)
  const [counts, setCounts] = useState<ChangeCounts | null>(null)
  const [summaryError, setSummaryError] = useState<string | null>(null)

  // Drag state for the histogram: indexes into the bucket list.
  const [dragAnchor, setDragAnchor] = useState<number | null>(null)
  const [dragEnd, setDragEnd] = useState<number | null>(null)
  const [hovered, setHovered] = useState<number | null>(null)

  const selectionKey = rangeKey(selection)

  // ── Activity for the histogram ────────────────────────────────────
  const activityRequest = useRef(0)
  useEffect(() => {
    const request = ++activityRequest.current
    // All of it: only hours with changes are returned, so this stays small.
    fetch(`/api/timeline/activity?root_id=${rootId}`)
      .then(async (response) => {
        if (!response.ok) throw new Error(response.statusText)
        return (await response.json()) as { points: ActivityPoint[] }
      })
      .then((data) => {
        if (request !== activityRequest.current) return
        setPoints(data.points)
        setActivityError(null)
        if (!granularityChosen.current) {
          // A root whose recent weeks are nearly empty is better
          // introduced by its whole history.
          const recentStart = nowSeconds() - DAY_BUCKETS * DAY
          const recentDays = new Set(
            data.points.filter((p) => p.time >= recentStart).map((p) => Math.floor(p.time / DAY))
          )
          setGranularity(recentDays.size >= 3 || data.points.length === 0 ? 'day' : 'all')
        }
      })
      .catch((error: unknown) => {
        if (request !== activityRequest.current) return
        setPoints([])
        setActivityError(error instanceof Error ? error.message : 'Could not load activity')
      })
  }, [rootId, refreshToken])

  // ── What the selection resolves to, and its net change counts ─────
  const summaryRequest = useRef(0)
  useEffect(() => {
    const request = ++summaryRequest.current
    if (!isRange(selection)) {
      setResolved(resolvePoint(selection, nowSeconds()))
      setCounts(null)
      setSummaryError(null)
      return
    }
    const params = new URLSearchParams({ root_id: rootId.toString(), ...rangeParams(selection) })
    fetch(`/api/timeline/summary?${params}`)
      .then(async (response) => {
        if (!response.ok) throw new Error(response.statusText)
        return (await response.json()) as { range: ResolvedRange; counts: ChangeCounts }
      })
      .then((data) => {
        if (request !== summaryRequest.current) return
        setResolved(data.range)
        setCounts(data.counts)
        setSummaryError(null)
      })
      .catch((error: unknown) => {
        if (request !== summaryRequest.current) return
        setCounts(null)
        setSummaryError(error instanceof Error ? error.message : 'Could not load changes')
      })
    // selectionKey stands in for `selection`, which is a new object on every change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [rootId, selectionKey, refreshToken])

  // ── Histogram ─────────────────────────────────────────────────────
  const buckets = useMemo(
    () => buildBuckets(points, granularity, nowSeconds()),
    // refreshToken moves "now" forward along with the data.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [points, granularity, refreshToken]
  )
  const maxTotal = Math.max(1, ...buckets.map(bucketTotal))

  const isSelected = useCallback(
    (index: number): boolean => {
      if (dragAnchor !== null && dragEnd !== null) {
        return index >= Math.min(dragAnchor, dragEnd) && index <= Math.max(dragAnchor, dragEnd)
      }
      if (!resolved || resolved.from === null) return false
      const bucket = buckets[index]
      return bucket.end > resolved.from && bucket.start < resolved.to
    },
    [dragAnchor, dragEnd, resolved, buckets]
  )

  const commitDrag = useCallback(() => {
    if (dragAnchor === null || dragEnd === null) return
    const first = buckets[Math.min(dragAnchor, dragEnd)]
    const last = buckets[Math.max(dragAnchor, dragEnd)]
    setDragAnchor(null)
    setDragEnd(null)
    setPresetId('custom')
    // A range that reaches the present stays open-ended, so it keeps
    // showing new changes.
    onSelectionChange({ from: first.start, to: last.end > nowSeconds() ? null : last.end })
  }, [dragAnchor, dragEnd, buckets, onSelectionChange])

  // Finish a drag that ends outside the histogram.
  useEffect(() => {
    if (dragAnchor === null) return
    window.addEventListener('mouseup', commitDrag)
    return () => window.removeEventListener('mouseup', commitDrag)
  }, [dragAnchor, commitDrag])

  // ── Handlers ──────────────────────────────────────────────────────
  const choosePreset = (id: PresetId) => {
    setPresetId(id)
    onSelectionChange(presetSelection(id))
  }

  const refresh = () => {
    // A preset is relative to now, so move it forward too.
    if (presetId !== 'custom') onSelectionChange(presetSelection(presetId))
    onRefresh()
  }

  const title =
    presetId !== 'custom'
      ? PRESETS.find((p) => p.id === presetId)?.label
      : isRange(selection)
        ? 'Changes'
        : 'State'

  return (
    <Collapsible open={open} onOpenChange={setOpen}>
      <div className="border border-border rounded-lg bg-muted/30">
        {/* What is being shown. Always visible. */}
        <div className="flex items-center gap-3 px-3 py-2">
          <CollapsibleTrigger asChild>
            <button
              className="flex items-center gap-2 min-w-0 text-left border-none bg-transparent p-0 cursor-pointer"
              aria-label="Change the time range"
            >
              <ChevronDown className={cn('h-4 w-4 flex-shrink-0 text-muted-foreground transition-transform', !open && '-rotate-90')} />
              <span className="text-sm font-medium whitespace-nowrap">{title}</span>
              {resolved && (
                <span className="text-sm text-muted-foreground truncate">{describeRange(resolved)}</span>
              )}
            </button>
          </CollapsibleTrigger>

          {counts && (
            <div className="flex items-center gap-3 text-sm whitespace-nowrap">
              <span className="inline-flex items-center gap-1" title="Items that did not exist at the start and exist at the end">
                <Plus className="h-3.5 w-3.5 text-green-500" />
                {formatCount(counts.added)} added
              </span>
              <span className="inline-flex items-center gap-1" title="Items that exist at both ends and differ">
                <Triangle className="h-3.5 w-3.5 text-blue-500" fill="currentColor" />
                {formatCount(counts.modified)} modified
              </span>
              <span className="inline-flex items-center gap-1" title="Items that existed at the start and do not exist at the end">
                <X className="h-3.5 w-3.5 text-red-500" />
                {formatCount(counts.deleted)} deleted
              </span>
              {/* Shown only when there are any: most ranges have none. */}
              {counts.ignored > 0 &&
                (onShowIgnored ? (
                  // Ignored items are hidden unless asked for; this count
                  // is where the user learns there are some, so it is
                  // also the way to see them.
                  <button
                    type="button"
                    className="text-muted-foreground underline decoration-dotted underline-offset-2 hover:text-foreground transition-colors"
                    title="Items that stopped being tracked because they came to match an ignore pattern. They are not counted as deleted. Click to show ignored items."
                    onClick={onShowIgnored}
                  >
                    {formatCount(counts.ignored)} ignored
                  </button>
                ) : (
                  <span
                    className="text-muted-foreground"
                    title="Items that stopped being tracked because they came to match an ignore pattern. They are not counted as deleted."
                  >
                    {formatCount(counts.ignored)} ignored
                  </span>
                ))}
              {counts.resumed > 0 && (
                <span
                  className="text-muted-foreground"
                  title="Items tracked again after being ignored. They are not counted as added."
                >
                  {formatCount(counts.resumed)} tracking resumed
                </span>
              )}
            </div>
          )}
          {summaryError && <span className="text-sm text-red-600">{summaryError}</span>}

          <div className="flex-1" />

          <Button
            variant="ghost"
            size="sm"
            className="h-6 w-6 p-0 flex-shrink-0"
            onClick={refresh}
            title="Read again, up to the present"
            aria-label="Refresh"
          >
            <RefreshCw className="h-3.5 w-3.5" />
          </Button>
        </div>

        <CollapsibleContent>
          <div className="border-t border-border px-3 py-3 space-y-3">
            {/* Presets */}
            <div className="flex flex-wrap items-center gap-1.5">
              {PRESETS.map((preset) => (
                <Button
                  key={preset.id}
                  size="sm"
                  variant={presetId === preset.id ? 'default' : 'outline'}
                  className="h-7"
                  onClick={() => choosePreset(preset.id)}
                >
                  {preset.label}
                </Button>
              ))}
            </div>

            {/* Activity histogram: drag across bars to choose a range. */}
            <div className="space-y-1">
              <div className="flex items-center justify-between text-xs text-muted-foreground">
                <span>
                  {hovered !== null
                    ? bucketLabel(buckets[hovered], granularity)
                    : 'Drag across the bars to choose a range'}
                </span>
                <span className="inline-flex items-center gap-1">
                  {(['all', 'day', 'hour'] as Granularity[]).map((g) => (
                    <button
                      key={g}
                      type="button"
                      className={cn(
                        'px-1.5 py-0.5 rounded cursor-pointer',
                        granularity === g ? 'bg-accent text-foreground font-medium' : 'hover:bg-accent/50'
                      )}
                      onClick={() => {
                        granularityChosen.current = true
                        setGranularity(g)
                      }}
                    >
                      {g === 'all' ? 'All history' : g === 'day' ? `${DAY_BUCKETS} days` : `${HOUR_BUCKETS} hours`}
                    </button>
                  ))}
                </span>
              </div>

              {activityError ? (
                <div className="h-16 flex items-center justify-center text-sm text-red-600">{activityError}</div>
              ) : (
                <div
                  className="flex items-end gap-px h-16 select-none"
                  onMouseLeave={() => setHovered(null)}
                  role="group"
                  aria-label="Activity over time"
                >
                  {buckets.map((bucket, index) => {
                    const total = bucketTotal(bucket)
                    // Square-root scale: one very busy period must not flatten the rest.
                    const height = total === 0 ? 0 : Math.max(6, Math.round(Math.sqrt(total / maxTotal) * 100))
                    return (
                      <div
                        key={bucket.start}
                        className={cn(
                          'flex-1 h-full flex items-end cursor-crosshair rounded-sm',
                          isSelected(index) ? 'bg-primary/15' : 'hover:bg-accent'
                        )}
                        title={bucketLabel(bucket, granularity)}
                        onMouseDown={(event) => {
                          event.preventDefault()
                          setDragAnchor(index)
                          setDragEnd(index)
                        }}
                        onMouseEnter={() => {
                          setHovered(index)
                          if (dragAnchor !== null) setDragEnd(index)
                        }}
                      >
                        <div
                          className={cn(
                            'w-full rounded-sm',
                            isSelected(index) ? 'bg-primary' : 'bg-muted-foreground/50'
                          )}
                          style={{ height: `${height}%` }}
                        />
                      </div>
                    )
                  })}
                </div>
              )}

              <div className="flex justify-between text-xs text-muted-foreground">
                <span>{buckets.length > 0 && format(new Date(buckets[0].start * 1000), granularity === 'hour' ? 'MMM d, HH:00' : 'MMM d yyyy')}</span>
                <span>now</span>
              </div>
            </div>

            <p className="text-xs text-muted-foreground">
              A range compares its two ends. The past is recorded by the hour, so a start, and an end
              that is not now, fall on the hour.
            </p>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  )
}
