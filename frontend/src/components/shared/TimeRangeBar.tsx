import { useState, useEffect, useLayoutEffect, useMemo, useRef, useCallback, type MouseEvent as ReactMouseEvent } from 'react'
import { ArrowLeft, ArrowRight, ChevronDown, Plus, RefreshCw, ScanSearch, Triangle, X } from 'lucide-react'
import { Button } from '@/components/ui/button'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { MomentField } from '@/components/shared/MomentField'
import { cn } from '@/lib/utils'
import { formatCount } from '@/lib/formatUtils'
import {
  allHistory,
  bucketTotal,
  buildStrip,
  describeActivity,
  indexAt,
  isAllHistory,
  nameSpan,
  pagePeriod,
  tickLabel,
  unitFor,
  type ActivityPoint,
  type Period,
  type Unit,
} from '@/lib/activityWindow'
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

/** Mirror of the server's rule, for a point in time, where no request is needed. */
function resolvePoint(selection: TimeRangeSelection, now: number): ResolvedRange {
  if (selection.to === null || selection.to >= now) {
    return { from: null, to: now, to_is_now: true }
  }
  return { from: null, to: selection.to - (selection.to % HOUR), to_is_now: false }
}

const UNITS: { id: Unit; label: string; seconds: number }[] = [
  { id: 'hour', label: 'Hours', seconds: HOUR },
  { id: 'day', label: 'Days', seconds: DAY },
  { id: 'month', label: 'Months', seconds: 30 * DAY },
  { id: 'year', label: 'Years', seconds: 365 * DAY },
]

/** How wide one bar is, gap included, in pixels. */
const BAR_WIDTH: Record<Unit, number> = { hour: 10, day: 12, month: 22, year: 44 }

/** Heights, in pixels: the bars, the space over them, and the labels under them. */
const BARS_HEIGHT = 96
const BARS_TOP = 8
const AXIS_HEIGHT = 20
const PLOT_HEIGHT = BARS_TOP + BARS_HEIGHT

/** While dragging, the strip scrolls when the pointer is this close to an edge. */
const EDGE_ZONE = 28
const EDGE_STEP = 14

/** The narrowest and widest a bar is drawn when the view is fitted to a span. */
const MIN_FIT_BAR = 4
const MAX_FIT_BAR = 48

/**
 * The view that makes a span fill most of the strip: the finest
 * resolution whose bars are still wide enough to tell apart, and the
 * bar width at which the span then takes up 85% of the width.
 */
function fitView(period: Period, width: number): { unit: Unit; barWidth: number } {
  if (width <= 0) {
    const unit = unitFor(period)
    return { unit, barWidth: BAR_WIDTH[unit] }
  }
  const span = Math.max(period.end - period.start, HOUR)
  for (const unit of UNITS) {
    const each = (width * 0.85) / (span / unit.seconds)
    if (each >= MIN_FIT_BAR) return { unit: unit.id, barWidth: Math.min(MAX_FIT_BAR, Math.floor(each)) }
  }
  return { unit: 'year', barWidth: MIN_FIT_BAR }
}

/** What a drag on the strip is doing. */
type Drag =
  // Choosing a new range, from the bar the drag began on.
  | { kind: 'new'; anchor: number }
  // Moving one end of the range; the other stays at `fixed`.
  | { kind: 'start' | 'end'; fixed: number }

const STEP_ARROW =
  'inline-flex h-7 w-7 items-center justify-center rounded-full bg-primary text-primary-foreground shadow-sm ' +
  'transition-all cursor-pointer hover:brightness-110 active:scale-95 ' +
  'disabled:cursor-default disabled:bg-muted disabled:text-muted-foreground/40 disabled:shadow-none disabled:hover:brightness-100'

/**
 * The time control for a root.
 *
 * Two things are kept apart. The selection is the range the tree
 * shows: the tree as it stood at the range's end, with what changed
 * since its start marked. The view is the stretch of history the bars
 * happen to show, and at what resolution. Moving the view never
 * changes the selection.
 *
 * The bars are one continuous strip, from the first recorded change to
 * now, that scrolls sideways. Each bar is an hour, a day, a month or a
 * year, as chosen. The selection is a shaded band over the strip with
 * a handle at each end. Clicking a bar selects it, dragging across
 * bars selects them all, and dragging a handle moves one end; a drag
 * that reaches the edge scrolls the strip, so a range can be as long
 * as history. The two ends are also shown as dates that can be edited
 * in place; the band and the dates are the same range and each follows
 * the other.
 */
export function TimeRangeBar({ rootId, selection, onSelectionChange, initialPreset, refreshToken, onRefresh, onShowIgnored }: TimeRangeBarProps) {
  const [open, setOpen] = useState(false)
  const [presetId, setPresetId] = useState<PresetId | 'custom'>(initialPreset ?? 'custom')

  const [points, setPoints] = useState<ActivityPoint[]>([])
  const [activityLoaded, setActivityLoaded] = useState(false)
  const [activityError, setActivityError] = useState<string | null>(null)

  const [resolved, setResolved] = useState<ResolvedRange | null>(null)
  const [counts, setCounts] = useState<ChangeCounts | null>(null)
  const [summaryError, setSummaryError] = useState<string | null>(null)

  const selectionKey = rangeKey(selection)
  const comparing = isRange(selection)

  // ── Activity for the bars ─────────────────────────────────────────
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
        setActivityLoaded(true)
      })
      .catch((error: unknown) => {
        if (request !== activityRequest.current) return
        setPoints([])
        setActivityError(error instanceof Error ? error.message : 'Could not load activity')
        setActivityLoaded(true)
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

  // ── The selection, as a span of time ──────────────────────────────
  // refreshToken moves "now" forward along with the data.
  // eslint-disable-next-line react-hooks/exhaustive-deps
  const now = useMemo(() => nowSeconds(), [points, selectionKey, refreshToken])
  const all = useMemo(() => allHistory(points, now), [points, now])

  // A start is on the hour, as the server will make it. A selection
  // with no start is a single moment: its span is empty.
  const selected: Period = useMemo(() => {
    const end = selection.to === null ? now : Math.min(selection.to, now)
    const start = selection.from !== null ? Math.min(selection.from - (selection.from % HOUR), end) : end
    return { start, end }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectionKey, now])
  const endsNow = selected.end >= now - 60

  // ── The view: a strip of bars, and where it is scrolled to ────────
  const [unit, setUnit] = useState<Unit>(() => {
    const present = nowSeconds()
    return selection.from === null ? 'day' : unitFor({ start: selection.from, end: selection.to ?? present })
  })
  // A bar width other than the resolution's usual one, set by fitting the view to a span.
  const [fittedWidth, setFittedWidth] = useState<number | null>(null)
  const barWidth = fittedWidth ?? BAR_WIDTH[unit]

  // The strip's scrolling element. Held in state, not a ref, because it
  // only exists while the control is open.
  const [scroller, setScroller] = useState<HTMLDivElement | null>(null)
  const [width, setWidth] = useState(0)
  const [scrollLeft, setScrollLeft] = useState(0)

  useEffect(() => {
    if (!scroller) {
      setWidth(0)
      return
    }
    const observer = new ResizeObserver(() => setWidth(scroller.clientWidth))
    observer.observe(scroller)
    return () => observer.disconnect()
  }, [scroller])

  const bars = useMemo(
    () => buildStrip(points, all, now, unit, Math.ceil(width / barWidth)),
    [points, all, now, unit, width, barWidth]
  )
  const maxTotal = useMemo(() => Math.max(1, ...bars.map(bucketTotal)), [bars])
  const stripWidth = bars.length * barWidth

  /** Where a moment is on the strip, in pixels from its left end. */
  const xOf = useCallback(
    (time: number): number => {
      if (bars.length === 0 || time <= bars[0].start) return 0
      if (time >= bars[bars.length - 1].end) return bars.length * barWidth
      const index = indexAt(bars, time)
      const bar = bars[index]
      return (index + (time - bar.start) / (bar.end - bar.start)) * barWidth
    },
    [bars, barWidth]
  )

  /** The moment at a position on the strip. */
  const timeAt = useCallback(
    (x: number): number => {
      if (bars.length === 0) return 0
      const index = Math.min(bars.length - 1, Math.max(0, Math.floor(x / barWidth)))
      const bar = bars[index]
      const within = Math.min(1, Math.max(0, x / barWidth - index))
      return bar.start + within * (bar.end - bar.start)
    },
    [bars, barWidth]
  )

  // The moment the view is centred on. The strip is rebuilt when the
  // resolution, the width or the data changes, and the view is then
  // put back on this moment. `wanted` is a moment to go to instead.
  const centre = useRef<number | null>(null)
  const wanted = useRef<number | null>(selected.start + (selected.end - selected.start) / 2)
  const stripShape = useRef('')
  const [moves, setMoves] = useState(0)

  const centreOn = (time: number) => {
    wanted.current = time
    setMoves((count) => count + 1)
  }

  useLayoutEffect(() => {
    const element = scroller
    if (!element || width === 0 || bars.length === 0 || !activityLoaded) {
      if (width === 0) stripShape.current = ''
      return
    }
    const shape = `${unit}:${barWidth}:${bars[0].start}:${width}`
    // Closing the control sets the width to 0, so reopening it is a new shape too.
    const target = wanted.current ?? (shape !== stripShape.current ? centre.current : null)
    stripShape.current = shape
    wanted.current = null
    if (target === null) return
    element.scrollLeft = xOf(target) - width / 2
    centre.current = target
    setScrollLeft(element.scrollLeft)
  }, [scroller, bars, unit, barWidth, width, xOf, moves, activityLoaded])

  const onScroll = () => {
    const element = scroller
    if (!element) return
    setScrollLeft(element.scrollLeft)
    centre.current = timeAt(element.scrollLeft + width / 2)
  }

  // ── Changing the selection ────────────────────────────────────────
  // What a drag would select if it ended now. Shown in place of the
  // selection while dragging.
  const [draft, setDraft] = useState<Period | null>(null)
  const [hovered, setHovered] = useState<number | null>(null)
  const shown = draft ?? selected
  const bandLeft = xOf(shown.start)
  const bandRight = Math.max(xOf(shown.end), bandLeft + 2)

  /** Make a span the selection. One that reaches the present stays open-ended, so it keeps showing new changes. */
  const select = useCallback(
    (period: Period) => {
      setPresetId('custom')
      onSelectionChange({ from: period.start, to: period.end >= now ? null : period.end })
    },
    [onSelectionChange, now]
  )

  /** Scroll a span into view, unless it is there already or is wider than the view and partly in it. */
  const reveal = (period: Period) => {
    if (!scroller || width === 0) return
    const left = xOf(period.start)
    const right = xOf(period.end)
    const inView = right > scroller.scrollLeft && left < scroller.scrollLeft + width
    const whole = left >= scroller.scrollLeft && right <= scroller.scrollLeft + width
    if (whole || (inView && right - left > width)) return
    scroller.scrollTo({ left: (left + right) / 2 - width / 2, behavior: 'smooth' })
  }

  // Bring a selection made elsewhere (stepping, the URL) into view.
  useEffect(() => {
    if (wanted.current === null) reveal(selected)
    // Only when the selection changes; scrolling away from it afterwards is allowed.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectionKey])

  /** The bar under a pointer position, wherever the strip is scrolled to. */
  const barUnder = (clientX: number): number => {
    const element = scroller
    if (!element) return 0
    const x = clientX - element.getBoundingClientRect().left + element.scrollLeft
    return Math.min(bars.length - 1, Math.max(0, Math.floor(x / barWidth)))
  }

  const startDrag = (drag: Drag, event: ReactMouseEvent) => {
    if (event.button !== 0 || bars.length === 0) return
    event.preventDefault()
    let pointerX = event.clientX
    let current: Period | null = null

    const update = () => {
      const index = barUnder(pointerX)
      if (drag.kind === 'new') {
        const first = bars[Math.min(drag.anchor, index)]
        const last = bars[Math.max(drag.anchor, index)]
        current = { start: first.start, end: Math.max(Math.min(last.end, now), first.start + 1) }
      } else if (drag.kind === 'start') {
        current = { start: Math.min(bars[index].start, drag.fixed - HOUR), end: drag.fixed }
      } else {
        current = { start: drag.fixed, end: Math.max(Math.min(bars[index].end, now), drag.fixed + HOUR) }
      }
      setDraft(current)
    }

    // Scroll while the pointer is held at, or past, either edge.
    const edgeTimer = window.setInterval(() => {
      const element = scroller
      if (!element) return
      const box = element.getBoundingClientRect()
      const before = element.scrollLeft
      if (pointerX < box.left + EDGE_ZONE) element.scrollLeft -= EDGE_STEP
      else if (pointerX > box.right - EDGE_ZONE) element.scrollLeft += EDGE_STEP
      if (element.scrollLeft !== before) update()
    }, 30)

    const move = (moved: MouseEvent) => {
      pointerX = moved.clientX
      update()
    }
    const finish = () => {
      window.clearInterval(edgeTimer)
      window.removeEventListener('mousemove', move)
      window.removeEventListener('mouseup', finish)
      setDraft(null)
      if (current) select(current)
    }
    // Attached here, not in an effect, so a quick click cannot finish
    // before anything is listening.
    window.addEventListener('mousemove', move)
    window.addEventListener('mouseup', finish)
    update()
  }

  // ── Editing the two ends as dates ─────────────────────────────────
  // The band follows every keystroke, but the tree is only read again
  // once the typing pauses or the field is left.
  const editTimer = useRef<number | null>(null)
  const editPending = useRef<Period | null>(null)
  // The end is being edited although it is "now": show it as a date.
  const [endPinned, setEndPinned] = useState(false)

  const finishEdit = () => {
    if (editTimer.current !== null) window.clearTimeout(editTimer.current)
    editTimer.current = null
    const period = editPending.current
    editPending.current = null
    if (!period) return
    setDraft(null)
    if (comparing) {
      select(period)
    } else {
      setPresetId('custom')
      onSelectionChange({ from: null, to: period.end >= now ? null : period.end })
    }
  }
  // The timer calls whichever `finishEdit` is current when it fires.
  const finishEditRef = useRef(finishEdit)
  useEffect(() => {
    finishEditRef.current = finishEdit
  })
  useEffect(
    () => () => {
      if (editTimer.current !== null) window.clearTimeout(editTimer.current)
    },
    []
  )

  const edit = (period: Period) => {
    editPending.current = period
    setDraft(period)
    reveal(period)
    if (editTimer.current !== null) window.clearTimeout(editTimer.current)
    editTimer.current = window.setTimeout(() => finishEditRef.current(), 700)
  }

  const thisHour = now - (now % HOUR)

  /** A new start: no earlier than the strip, and at least an hour before the end. */
  const editStart = (time: number) => {
    const latest = shown.end - HOUR - ((shown.end - HOUR) % HOUR)
    const earliest = bars.length > 0 ? bars[0].start : time
    edit({ start: Math.max(earliest, Math.min(time, latest)), end: shown.end })
  }

  /** A new end: at least an hour after the start. Anything past the current hour is "now". */
  const editEnd = (time: number) => {
    const end = time > thisHour ? now : comparing ? Math.max(time, shown.start + HOUR) : time
    edit({ start: comparing ? shown.start : end, end })
  }

  const shownEndsNow = shown.end >= now - 60

  const previous = comparing ? pagePeriod(selected, -1, all, now) : null
  const next = comparing ? pagePeriod(selected, 1, all, now) : null

  // ── Changing the view ─────────────────────────────────────────────
  const selectionInView = bandRight > scrollLeft && bandLeft < scrollLeft + width
  const selectionMiddle = selected.start + (selected.end - selected.start) / 2

  /** Another resolution, keeping the selection in view if it was, and otherwise the same stretch of history. */
  const changeUnit = (next: Unit) => {
    if (next === unit && fittedWidth === null) return
    setUnit(next)
    setFittedWidth(null)
    centreOn(selectionInView ? selectionMiddle : (centre.current ?? selectionMiddle))
  }

  /** Fit the view to a span, so that it fills most of the strip. */
  const zoomTo = (period: Period) => {
    const view = fitView(period, width)
    setUnit(view.unit)
    setFittedWidth(view.barWidth)
    centreOn(period.start + (period.end - period.start) / 2)
  }

  // ── Handlers ──────────────────────────────────────────────────────
  const choosePreset = (id: PresetId) => {
    const chosen = presetSelection(id)
    setPresetId(id)
    onSelectionChange(chosen)
    const present = nowSeconds()
    zoomTo({ start: chosen.from ?? present, end: present })
  }

  const chooseAllHistory = () => {
    select(all)
    zoomTo(all)
  }

  const refresh = () => {
    // A preset is relative to now, so move it forward too.
    if (presetId !== 'custom') onSelectionChange(presetSelection(presetId))
    onRefresh()
  }

  const title =
    presetId !== 'custom'
      ? PRESETS.find((p) => p.id === presetId)?.label
      : comparing
        ? 'Changes'
        : endsNow
          ? 'Current state'
          : 'Point in time'

  // Only the bars in view, and a few either side, are drawn.
  const firstDrawn = Math.max(0, Math.floor(scrollLeft / barWidth) - 10)
  const lastDrawn = Math.min(bars.length - 1, Math.ceil((scrollLeft + width) / barWidth) + 10)
  const drawn = bars.slice(firstDrawn, lastDrawn + 1)

  // The span being dragged across, or else the bar under the pointer.
  const pointed = draft ?? (hovered !== null && bars[hovered] ? bars[hovered] : null)
  const pointerLabel = pointed
    ? `${nameSpan({ start: pointed.start, end: pointed.end }, now)} (${describeActivity(points, pointed)})`
    : ''
  const allSelected = comparing && isAllHistory(selected, all)

  return (
    <Collapsible
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        // A field that is removed while it has the focus is never told it lost it.
        if (!next) setEndPinned(false)
      }}
    >
      <div className="border border-border rounded-lg bg-muted/30">
        {/* What is being shown. Always visible. */}
        <div className="flex items-center gap-3 px-3 py-2">
          <CollapsibleTrigger asChild>
            <button
              className="flex items-center gap-2 min-w-0 text-left border-none bg-transparent p-0 cursor-pointer"
              aria-label="Show or hide the time controls"
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
          <div className="border-t border-border px-4 pt-3 pb-4 space-y-3">
            {/* The selection, in words, and the ways to move it. */}
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
              <div className="flex items-center gap-3">
                <button
                  type="button"
                  className={STEP_ARROW}
                  disabled={!previous}
                  onClick={() => previous && select(previous)}
                  title="Earlier: the same length of time, just before this"
                  aria-label="Step the range earlier"
                >
                  <ArrowLeft className="h-3.5 w-3.5" strokeWidth={2.5} />
                </button>
                <div className="flex items-center gap-x-4 gap-y-1.5 flex-wrap text-sm">
                  {comparing && (
                    <span className="inline-flex items-center gap-2">
                      <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">From</span>
                      <MomentField value={shown.start} onChange={editStart} onDone={finishEdit} label="Start of the range" />
                    </span>
                  )}
                  <span className="inline-flex items-center gap-2">
                    <span className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                      {comparing ? 'To' : 'As of'}
                    </span>
                    {shownEndsNow && !endPinned ? (
                      // "Now" is not a date. It becomes one, at the current
                      // hour, when the user goes to edit it; nothing changes
                      // until they do.
                      <button
                        type="button"
                        className="rounded-md border border-input bg-background px-2 py-1 text-sm font-semibold cursor-text transition-colors hover:bg-accent"
                        title="Follows the present. Click to set a date and hour instead."
                        onClick={() => setEndPinned(true)}
                      >
                        now
                      </button>
                    ) : (
                      <>
                        <MomentField
                          value={shownEndsNow ? thisHour : shown.end}
                          onChange={editEnd}
                          onDone={() => {
                            finishEdit()
                            setEndPinned(false)
                          }}
                          label={comparing ? 'End of the range' : 'The moment shown'}
                          focusOnMount={endPinned}
                        />
                        {!shownEndsNow && (
                          <button
                            type="button"
                            className="rounded-full px-2 py-0.5 text-xs text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground"
                            title="End at the present, and keep following it"
                            onClick={() => editEnd(now)}
                          >
                            now
                          </button>
                        )}
                      </>
                    )}
                  </span>
                </div>
                <button
                  type="button"
                  className={STEP_ARROW}
                  disabled={!next}
                  onClick={() => next && select(next)}
                  title="Later: the same length of time, just after this"
                  aria-label="Step the range later"
                >
                  <ArrowRight className="h-3.5 w-3.5" strokeWidth={2.5} />
                </button>
              </div>

              <div className="flex items-center gap-1 text-xs">
                {PRESETS.filter((preset) => preset.span !== null).map((preset) => (
                  <button
                    key={preset.id}
                    type="button"
                    className={cn(
                      'rounded-full px-2.5 py-1 transition-colors cursor-pointer',
                      presetId === preset.id
                        ? 'bg-primary/15 text-foreground font-medium'
                        : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                    )}
                    onClick={() => choosePreset(preset.id)}
                  >
                    {preset.label}
                  </button>
                ))}
                <button
                  type="button"
                  className={cn(
                    'rounded-full px-2.5 py-1 transition-colors cursor-pointer',
                    allSelected
                      ? 'bg-primary/15 text-foreground font-medium'
                      : 'text-muted-foreground hover:bg-accent hover:text-foreground'
                  )}
                  onClick={chooseAllHistory}
                >
                  All history
                </button>
              </div>
            </div>

            <p className="text-xs text-muted-foreground">
              {comparing
                ? 'The tree is shown as it stood at the end of this range, with what changed since the start marked.'
                : 'The tree is shown as it stood at this moment. Nothing is marked as changed.'}
            </p>

            <div>
              {/* The view: its resolution, and what the pointer is over. */}
              <div className="mb-2 flex items-center gap-3">
                <div className="inline-flex rounded-lg bg-muted p-0.5" role="group" aria-label="What one bar stands for">
                  {UNITS.map((choice) => (
                    <button
                      key={choice.id}
                      type="button"
                      aria-pressed={unit === choice.id}
                      className={cn(
                        'rounded-md px-2.5 py-1 text-xs font-medium transition-all cursor-pointer',
                        unit === choice.id ? 'bg-background text-foreground shadow-sm' : 'text-muted-foreground hover:text-foreground'
                      )}
                      onClick={() => changeUnit(choice.id)}
                    >
                      {choice.label}
                    </button>
                  ))}
                </div>
                <div className="min-w-0 flex-1 truncate text-center text-xs text-muted-foreground">{pointerLabel}</div>
                <button
                  type="button"
                  className="inline-flex items-center gap-1 rounded-full px-2.5 py-1 text-xs text-muted-foreground transition-colors cursor-pointer hover:bg-accent hover:text-foreground disabled:cursor-default disabled:opacity-40 disabled:hover:bg-transparent"
                  disabled={!comparing}
                  onClick={() => zoomTo(selected)}
                  title="Fit the view to the selected range, so it fills most of the strip"
                >
                  <ScanSearch className="h-3.5 w-3.5" />
                  Zoom to selection
                </button>
              </div>

              {activityError ? (
                <div className="h-24 flex items-center justify-center text-sm text-red-600">{activityError}</div>
              ) : (
                <div className="relative">
                  <div
                    ref={setScroller}
                    className="overflow-x-auto overflow-y-hidden rounded-md bg-background/60 pb-1 [scrollbar-width:thin] [scrollbar-color:color-mix(in_oklab,var(--muted-foreground)_45%,transparent)_transparent]"
                    onScroll={onScroll}
                  >
                    <div
                      className="relative select-none cursor-crosshair"
                      style={{ width: stripWidth, height: PLOT_HEIGHT + AXIS_HEIGHT }}
                      role="group"
                      aria-label="Activity over time"
                      onMouseDown={(event) => startDrag({ kind: 'new', anchor: barUnder(event.clientX) }, event)}
                      onMouseMove={(event) => setHovered(barUnder(event.clientX))}
                      onMouseLeave={() => setHovered(null)}
                    >
                      {/* The selection, behind the bars. */}
                      <div
                        className="pointer-events-none absolute top-0 bg-primary/10"
                        style={{ left: bandLeft, width: bandRight - bandLeft, height: PLOT_HEIGHT }}
                      />

                      {hovered !== null && !draft && (
                        <div
                          className="pointer-events-none absolute top-0 bg-foreground/5"
                          style={{ left: hovered * barWidth, width: barWidth, height: PLOT_HEIGHT }}
                        />
                      )}

                      {drawn.map((bar, offset) => {
                        const index = firstDrawn + offset
                        const total = bucketTotal(bar)
                        const label = tickLabel(bar, unit)
                        // Square-root scale: one very busy bar must not flatten the rest.
                        const height = total === 0 ? 0 : Math.max(4, Math.round(Math.sqrt(total / maxTotal) * BARS_HEIGHT))
                        return (
                          <div key={bar.start} className="pointer-events-none absolute top-0 bottom-0" style={{ left: index * barWidth, width: barWidth }}>
                            {label !== null && (
                              <>
                                {unit !== 'year' && <div className="absolute left-0 top-0 bottom-0 w-px bg-border/70" />}
                                <div
                                  className="absolute left-1 whitespace-nowrap text-[11px] leading-none text-muted-foreground"
                                  style={{ top: PLOT_HEIGHT + 6 }}
                                >
                                  {label}
                                </div>
                              </>
                            )}
                            {height > 0 && (
                              <div
                                className="absolute left-px right-px rounded-t-sm bg-gradient-to-t from-primary/60 to-primary"
                                style={{ bottom: AXIS_HEIGHT, height }}
                              />
                            )}
                          </div>
                        )
                      })}

                      <div className="pointer-events-none absolute left-0 right-0 h-px bg-border" style={{ top: PLOT_HEIGHT }} />

                      {/* The selection's two ends, in front of the bars. With
                          a range, each end is a handle that can be dragged. */}
                      {[
                        { kind: 'start' as const, x: bandLeft, fixed: shown.end },
                        { kind: 'end' as const, x: bandRight, fixed: shown.start },
                      ].map((edge) =>
                        !comparing && edge.kind === 'start' ? null : (
                          <div
                            key={edge.kind}
                            className={cn(
                              'absolute top-0 flex w-3 -translate-x-1/2 items-center justify-center',
                              comparing ? 'cursor-ew-resize' : 'pointer-events-none'
                            )}
                            style={{ left: edge.x, height: PLOT_HEIGHT }}
                            title={comparing ? (edge.kind === 'start' ? 'Drag to move the start' : 'Drag to move the end') : undefined}
                            onMouseDown={(event) => {
                              event.stopPropagation()
                              startDrag({ kind: edge.kind, fixed: edge.fixed }, event)
                            }}
                            onMouseMove={(event) => event.stopPropagation()}
                          >
                            <div className="absolute inset-y-0 left-1/2 w-0.5 -translate-x-1/2 bg-primary" />
                            {/* The grips need room: on a very narrow range they would cover it. */}
                            {comparing && bandRight - bandLeft >= 24 && (
                              <div className="relative h-7 w-1.5 rounded-full bg-primary shadow-sm ring-2 ring-background" />
                            )}
                          </div>
                        )
                      )}
                    </div>
                  </div>

                  {/* The selection is out of view: the way back to it. */}
                  {width > 0 && !selectionInView && !draft && (
                    <button
                      type="button"
                      className={cn(
                        'absolute top-1.5 inline-flex items-center gap-1 rounded-full bg-primary px-2.5 py-1 text-xs font-medium text-primary-foreground shadow-sm cursor-pointer hover:brightness-110',
                        bandRight <= scrollLeft ? 'left-1.5' : 'right-1.5'
                      )}
                      onClick={() => centreOn(selectionMiddle)}
                    >
                      {bandRight <= scrollLeft && <ArrowLeft className="h-3 w-3" />}
                      Selection
                      {bandRight > scrollLeft && <ArrowRight className="h-3 w-3" />}
                    </button>
                  )}
                </div>
              )}
            </div>

            <p className="text-center text-xs text-muted-foreground/80">
              Click a bar, or drag across several, to choose what the tree shows. Drag an end, or edit its date above, to adjust it. Scroll sideways to move through history.
            </p>
          </div>
        </CollapsibleContent>
      </div>
    </Collapsible>
  )
}
