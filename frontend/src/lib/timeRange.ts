import { format, isSameDay, isSameYear } from 'date-fns'

/**
 * What the user asked to look at. Times are Unix seconds.
 *
 * - `from === null`: a point in time (no comparison).
 * - `to === null`: now.
 *
 * The server adjusts these (a start, and an end in the past, move back
 * to the hour) and reports what it used as a `ResolvedRange`.
 */
export interface TimeRangeSelection {
  from: number | null
  to: number | null
}

/** The range the server actually used for a response. */
export interface ResolvedRange {
  from: number | null
  to: number
  to_is_now: boolean
}

export const HOUR = 3600
export const DAY = 24 * HOUR

export const CURRENT_STATE: TimeRangeSelection = { from: null, to: null }

export type PresetId = 'now' | 'hour' | 'day' | 'week' | 'month'

export interface Preset {
  id: PresetId
  label: string
  /** Seconds back from now for the range start; null for no comparison. */
  span: number | null
}

export const PRESETS: Preset[] = [
  { id: 'now', label: 'Current state', span: null },
  { id: 'hour', label: 'Last hour', span: HOUR },
  { id: 'day', label: 'Last 24 hours', span: DAY },
  { id: 'week', label: 'Last 7 days', span: 7 * DAY },
  { id: 'month', label: 'Last 30 days', span: 30 * DAY },
]

export const DEFAULT_PRESET: PresetId = 'day'

export function nowSeconds(): number {
  return Math.floor(Date.now() / 1000)
}

export function presetSelection(id: PresetId): TimeRangeSelection {
  const preset = PRESETS.find((p) => p.id === id)
  if (!preset || preset.span === null) return CURRENT_STATE
  return { from: nowSeconds() - preset.span, to: null }
}

/** Query parameters for the time API. Absent keys mean "not given". */
export function rangeParams(selection: TimeRangeSelection): Record<string, string> {
  const params: Record<string, string> = {}
  if (selection.from !== null) params.from = selection.from.toString()
  if (selection.to !== null) params.to = selection.to.toString()
  return params
}

/** Stable identity of a selection, for cache keys and effect dependencies. */
export function rangeKey(selection: TimeRangeSelection): string {
  return `${selection.from ?? ''}:${selection.to ?? ''}`
}

export function isRange(selection: TimeRangeSelection): boolean {
  return selection.from !== null
}

/** Read a selection from URL search parameters; null if none is present. */
export function selectionFromSearch(search: URLSearchParams): TimeRangeSelection | null {
  const parse = (name: string): number | null => {
    const raw = search.get(name)
    if (raw === null || raw === '') return null
    const value = Number.parseInt(raw, 10)
    return Number.isFinite(value) ? value : null
  }
  const from = parse('from')
  const to = parse('to')
  if (from === null && to === null) return null
  return { from, to }
}

/** "Nov 17, 10:00" — the year only when it is not the current one. */
export function formatMoment(seconds: number): string {
  const date = new Date(seconds * 1000)
  return isSameYear(date, new Date())
    ? format(date, 'MMM d, HH:mm')
    : format(date, 'MMM d yyyy, HH:mm')
}

/**
 * The range in words: "Now", "As of Nov 17, 10:00",
 * "Nov 16, 10:00 to now", "Nov 16, 10:00 to 14:00".
 */
export function describeRange(range: ResolvedRange): string {
  const end = range.to_is_now ? 'now' : formatMoment(range.to)
  if (range.from === null) {
    return range.to_is_now ? 'Now' : `As of ${end}`
  }
  if (!range.to_is_now && isSameDay(new Date(range.from * 1000), new Date(range.to * 1000))) {
    return `${formatMoment(range.from)} to ${format(new Date(range.to * 1000), 'HH:mm')}`
  }
  return `${formatMoment(range.from)} to ${end}`
}
