import { format, isSameDay, isSameYear, startOfDay, startOfMonth, startOfYear } from 'date-fns'
import { DAY, HOUR } from '@/lib/timeRange'

/** One hour in which a root changed, from `/api/timeline/activity`. */
export interface ActivityPoint {
  /** Start of the hour, Unix seconds. */
  time: number
  added: number
  modified: number
  deleted: number
}

/** The period the bars cover, which is also the period the tree shows. Unix seconds. */
export interface Period {
  start: number
  /** Exclusive. May be later than now for a period that is still running. */
  end: number
}

/** What one bar stands for. Every bar in a view is the same unit. */
export type Unit = 'hour' | 'day' | 'month' | 'year'

/** One bar. */
export interface Bucket {
  /** Start, inclusive, and end, exclusive, in Unix seconds. */
  start: number
  end: number
  added: number
  modified: number
  deleted: number
}

export function bucketTotal(bucket: Bucket): number {
  return bucket.added + bucket.modified + bucket.deleted
}

const seconds = (date: Date): number => Math.floor(date.getTime() / 1000)

/**
 * The unit a period is drawn in: the finest one that does not make more
 * bars than can be told apart. A week or less is drawn by hour, up to
 * about four months by day, up to ten years by month, and anything
 * longer by year.
 */
export function unitFor(period: Period): Unit {
  const span = period.end - period.start
  if (span <= 7 * DAY + HOUR) return 'hour'
  if (span <= 124 * DAY) return 'day'
  if (span <= 3660 * DAY) return 'month'
  return 'year'
}

/** The start of the calendar unit a moment falls in, in local time. */
function unitStart(time: number, unit: Unit): Date {
  const date = new Date(time * 1000)
  switch (unit) {
    case 'hour':
      return new Date(Math.floor(time / HOUR) * HOUR * 1000)
    case 'day':
      return startOfDay(date)
    case 'month':
      return startOfMonth(date)
    case 'year':
      return startOfYear(date)
  }
}

/** Move a date forward by `count` units, on calendar boundaries. */
function addUnits(date: Date, unit: Unit, count: number): Date {
  const next = new Date(date)
  switch (unit) {
    case 'hour':
      return new Date(date.getTime() + count * HOUR * 1000)
    case 'day':
      next.setDate(next.getDate() + count)
      return next
    case 'month':
      next.setMonth(next.getMonth() + count)
      return next
    case 'year':
      next.setFullYear(next.getFullYear() + count)
      return next
  }
}

/**
 * One bar per calendar unit across a period. Bars sit on calendar
 * boundaries (local midnight, the first of the month, January 1), so a
 * bar can reach outside the period at either end. Each active hour's
 * counts go to the bar it falls in.
 */
export function buildBuckets(points: ActivityPoint[], period: Period, unit: Unit): Bucket[] {
  const buckets: Bucket[] = []
  let cursor = unitStart(period.start, unit)
  while (seconds(cursor) < period.end) {
    const next = addUnits(cursor, unit, 1)
    buckets.push({ start: seconds(cursor), end: seconds(next), added: 0, modified: 0, deleted: 0 })
    cursor = next
  }

  // Points and buckets are both in time order.
  let index = 0
  for (const point of points) {
    while (index < buckets.length && point.time >= buckets[index].end) index++
    if (index >= buckets.length) break
    if (point.time >= buckets[index].start) {
      buckets[index].added += point.added
      buckets[index].modified += point.modified
      buckets[index].deleted += point.deleted
    }
  }
  return buckets
}

/**
 * The bars for the whole of history at one resolution: from the first
 * recorded change to the bar that holds now. If that is fewer than
 * `minBars`, empty bars are added before it, so the strip fills the
 * space it is drawn in.
 */
export function buildStrip(points: ActivityPoint[], all: Period, now: number, unit: Unit, minBars: number): Bucket[] {
  const bars = buildBuckets(points, { start: all.start, end: now + 1 }, unit)
  if (bars.length >= minBars || bars.length === 0) return bars
  const padding: Bucket[] = []
  let cursor = new Date(bars[0].start * 1000)
  for (let missing = minBars - bars.length; missing > 0; missing--) {
    const previous = addUnits(cursor, unit, -1)
    padding.push({ start: seconds(previous), end: seconds(cursor), added: 0, modified: 0, deleted: 0 })
    cursor = previous
  }
  return [...padding.reverse(), ...bars]
}

/** The index of the bar a moment falls in, clamped to the strip. */
export function indexAt(bars: Bucket[], time: number): number {
  let low = 0
  let high = bars.length - 1
  while (low < high) {
    const middle = (low + high) >> 1
    if (time >= bars[middle].end) low = middle + 1
    else high = middle
  }
  return low
}

/** Everything recorded: from the day of the first change to now. */
export function allHistory(points: ActivityPoint[], now: number): Period {
  const first = points.length > 0 ? points[0].time : now - 30 * DAY
  return { start: Math.min(seconds(startOfDay(new Date(first * 1000))), now - DAY), end: now }
}

export function isAllHistory(period: Period, all: Period): boolean {
  return period.start <= all.start && period.end >= all.end - HOUR
}

/**
 * If the period is exactly one calendar day, month or year, which. A
 * period that is still running (it began at the start of today, this
 * month or this year and reaches now) counts as that unit too.
 */
function calendarUnit(period: Period, now: number): Exclude<Unit, 'hour'> | null {
  for (const unit of ['day', 'month', 'year'] as const) {
    const start = unitStart(period.start, unit)
    if (seconds(start) !== period.start) continue
    const end = seconds(addUnits(start, unit, 1))
    if (period.end === end) return unit
    if (period.end >= now - HOUR && now < end) return unit
  }
  return null
}

/**
 * A span of time in words: "Tue, Nov 3 2026", "November 2026", "2026",
 * or for anything else its two ends, "Nov 3, 14:00 to Nov 5, 09:00".
 */
export function nameSpan(period: Period, now: number): string {
  const start = new Date(period.start * 1000)
  switch (calendarUnit(period, now)) {
    case 'day':
      return format(start, 'EEE, MMM d yyyy')
    case 'month':
      return format(start, 'MMMM yyyy')
    case 'year':
      return format(start, 'yyyy')
  }
  const end = new Date(period.end * 1000)
  const endsNow = period.end >= now - 60
  const sameYear = isSameYear(start, new Date(now * 1000)) && (endsNow || isSameYear(end, start))
  const onMidnights = seconds(startOfDay(start)) === period.start && (endsNow || seconds(startOfDay(end)) === period.end)
  if (onMidnights && !endsNow) {
    // Whole days: name the last day covered, not the midnight after it.
    const lastDay = new Date((period.end - 1) * 1000)
    const dayFormat = sameYear ? 'MMM d' : 'MMM d yyyy'
    return `${format(start, dayFormat)} to ${format(lastDay, dayFormat)}`
  }
  const momentFormat = sameYear ? 'MMM d, HH:mm' : 'MMM d yyyy, HH:mm'
  if (endsNow) return `${format(start, momentFormat)} to now`
  return isSameDay(start, new Date((period.end - 1) * 1000))
    ? `${format(start, momentFormat)} to ${format(end, 'HH:mm')}`
    : `${format(start, momentFormat)} to ${format(end, momentFormat)}`
}

/** As `nameSpan`, but everything recorded is "All history". */
export function describePeriod(period: Period, all: Period, now: number): string {
  return isAllHistory(period, all) ? 'All history' : nameSpan(period, now)
}

/**
 * The period before or after this one. A calendar day, month or year
 * moves by one of itself; anything else moves by its own length. Null
 * when there is nothing recorded in that direction.
 */
export function pagePeriod(period: Period, direction: -1 | 1, all: Period, now: number): Period | null {
  if (isAllHistory(period, all)) return null
  if (direction === 1 && period.end >= now - 60) return null
  if (direction === -1 && period.start <= all.start) return null

  const unit = calendarUnit(period, now)
  if (unit) {
    const start = addUnits(unitStart(period.start, unit), unit, direction)
    return { start: seconds(start), end: seconds(addUnits(start, unit, 1)) }
  }
  const span = period.end - period.start
  return { start: period.start + direction * span, end: period.end + direction * span }
}

/** What changed in a span, in words: "47 added, 373 modified, 17 deleted" or "no changes". */
export function describeActivity(points: ActivityPoint[], period: Period): string {
  let added = 0
  let modified = 0
  let deleted = 0
  for (const point of points) {
    if (point.time < period.start) continue
    if (point.time >= period.end) break
    added += point.added
    modified += point.modified
    deleted += point.deleted
  }
  if (added + modified + deleted === 0) return 'no changes'
  return `${added.toLocaleString()} added, ${modified.toLocaleString()} modified, ${deleted.toLocaleString()} deleted`
}

/**
 * The label under a bar that begins a larger unit: the day under its
 * first hour, the month under its first day, the year under January,
 * and every year. Null for every other bar.
 */
export function tickLabel(bar: Bucket, unit: Unit): string | null {
  const date = new Date(bar.start * 1000)
  switch (unit) {
    case 'hour':
      return date.getHours() === 0 ? format(date, 'EEE, MMM d') : null
    case 'day':
      return date.getDate() === 1 ? format(date, 'MMM yyyy') : null
    case 'month':
      return date.getMonth() === 0 ? format(date, 'yyyy') : null
    case 'year':
      return format(date, 'yyyy')
  }
}
