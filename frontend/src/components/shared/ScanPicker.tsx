import { useState, useEffect, useRef, useCallback } from 'react'
import { format } from 'date-fns'
import { Check } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Calendar, CalendarDayButton } from '@/components/ui/calendar'
import { cn } from '@/lib/utils'
import { formatScanDate, formatTime } from '@/lib/dateUtils'
import type { DayButton } from 'react-day-picker'

interface ScanPickerProps {
  rootId: number
  onScanResolved: (scanId: number, startedAt: number) => void
  onNoScan: () => void
}

interface ScanSummary {
  scan_id: number
  started_at: number
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
}

function formatChanges(add: number | null, modify: number | null, del: number | null): string {
  const parts: string[] = []
  if (add && add > 0) parts.push(`${add} ${add === 1 ? 'add' : 'adds'}`)
  if (modify && modify > 0) parts.push(`${modify} ${modify === 1 ? 'mod' : 'mods'}`)
  if (del && del > 0) parts.push(`${del} ${del === 1 ? 'del' : 'dels'}`)
  return parts.length > 0 ? parts.join(', ') : 'no changes'
}

export function ScanPicker({ rootId, onScanResolved, onNoScan }: ScanPickerProps) {
  const [selectedDate, setSelectedDate] = useState<Date | undefined>(undefined)

  // Calendar scan date highlighting
  const [scanDates, setScanDates] = useState<Set<string>>(new Set())
  const [displayMonth, setDisplayMonth] = useState<Date>(new Date())

  // Scans for selected date
  const [dateScans, setDateScans] = useState<ScanSummary[]>([])
  const [loadingDateScans, setLoadingDateScans] = useState(false)

  // Resolved scan (the final selection)
  const [resolvedScanId, setResolvedScanId] = useState<number | null>(null)
  const [resolvedStartedAt, setResolvedStartedAt] = useState<number | null>(null)
  const [resolving, setResolving] = useState(false)
  const [isFallback, setIsFallback] = useState(false)

  // Stale request tracking
  const resolveIdRef = useRef(0)
  const scanDatesIdRef = useRef(0)
  const dateScansIdRef = useRef(0)

  // ── Fetch scan dates for calendar highlighting ─────────────────────
  const fetchScanDates = useCallback(
    async (month: Date) => {
      const currentId = ++scanDatesIdRef.current
      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          year: month.getFullYear().toString(),
          month: (month.getMonth() + 1).toString(),
        })
        const response = await fetch(`/api/scans/scan_dates?${params}`)
        if (currentId !== scanDatesIdRef.current) return
        if (response.ok) {
          const data = (await response.json()) as { dates: string[] }
          setScanDates(new Set(data.dates))
        }
      } catch {
        if (currentId !== scanDatesIdRef.current) return
      }
    },
    [rootId]
  )

  // ── Fetch scans for a specific date ────────────────────────────────
  const fetchScansForDate = useCallback(
    async (date: Date, autoSelectScanId?: number) => {
      const currentId = ++dateScansIdRef.current
      setLoadingDateScans(true)
      setDateScans([])
      setIsFallback(false)

      const dateStr = format(date, 'yyyy-MM-dd')

      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          date: dateStr,
        })
        const response = await fetch(`/api/scans/by_date?${params}`)
        if (currentId !== dateScansIdRef.current) return

        if (response.ok) {
          const data = (await response.json()) as { scans: ScanSummary[] }

          if (data.scans.length > 0) {
            setDateScans(data.scans)
            const target = autoSelectScanId
              ? data.scans.find((s) => s.scan_id === autoSelectScanId) ?? data.scans[0]
              : data.scans[0]
            setResolvedScanId(target.scan_id)
            setResolvedStartedAt(target.started_at)
            onScanResolved(target.scan_id, target.started_at)
          } else {
            setDateScans([])
            const resolveParams = new URLSearchParams({
              root_id: rootId.toString(),
              date: dateStr,
            })
            const resolveResponse = await fetch(`/api/scans/resolve?${resolveParams}`)
            if (currentId !== dateScansIdRef.current) return

            if (resolveResponse.ok) {
              const resolveData = (await resolveResponse.json()) as {
                scan_id: number
                started_at: number
              }
              setResolvedScanId(resolveData.scan_id)
              setResolvedStartedAt(resolveData.started_at)
              setIsFallback(true)
              onScanResolved(resolveData.scan_id, resolveData.started_at)
            } else {
              setResolvedScanId(null)
              setResolvedStartedAt(null)
              onNoScan()
            }
          }
        }
      } catch {
        if (currentId !== dateScansIdRef.current) return
        onNoScan()
      } finally {
        if (currentId === dateScansIdRef.current) {
          setLoadingDateScans(false)
        }
      }
    },
    [rootId, onScanResolved, onNoScan]
  )

  // ── Jump to latest scan ────────────────────────────────────────────
  const jumpToLatest = useCallback(async () => {
    const currentId = ++resolveIdRef.current
    setResolving(true)

    try {
      const params = new URLSearchParams({ root_id: rootId.toString() })
      const response = await fetch(`/api/scans/resolve?${params}`)

      if (currentId !== resolveIdRef.current) return

      if (response.ok) {
        const data = (await response.json()) as { scan_id: number; started_at: number }
        const scanDate = new Date(data.started_at * 1000)

        setDisplayMonth(new Date(scanDate.getFullYear(), scanDate.getMonth(), 1))
        setSelectedDate(new Date(scanDate.getFullYear(), scanDate.getMonth(), scanDate.getDate()))

        setResolvedScanId(data.scan_id)
        setResolvedStartedAt(data.started_at)
        setIsFallback(false)
        onScanResolved(data.scan_id, data.started_at)

        const monthDate = new Date(scanDate.getFullYear(), scanDate.getMonth(), 1)
        fetchScanDates(monthDate)
        const dayDate = new Date(scanDate.getFullYear(), scanDate.getMonth(), scanDate.getDate())
        fetchScansForDate(dayDate, data.scan_id)
      } else {
        setResolvedScanId(null)
        setResolvedStartedAt(null)
        setSelectedDate(undefined)
        setDateScans([])
        onNoScan()
      }
    } catch {
      if (currentId !== resolveIdRef.current) return
      setResolvedScanId(null)
      setResolvedStartedAt(null)
      onNoScan()
    } finally {
      if (currentId === resolveIdRef.current) {
        setResolving(false)
      }
    }
  }, [rootId, onScanResolved, onNoScan, fetchScanDates, fetchScansForDate])

  // ── Initialize on mount / rootId change ────────────────────────────
  useEffect(() => {
    setScanDates(new Set())
    setDateScans([])
    setSelectedDate(undefined)
    setIsFallback(false)
    jumpToLatest()
  }, [rootId, jumpToLatest])

  // ── Fetch scan dates when calendar month changes ───────────────────
  useEffect(() => {
    fetchScanDates(displayMonth)
  }, [displayMonth, fetchScanDates])

  // ── Handlers ───────────────────────────────────────────────────────
  const handleDateSelect = (date: Date | undefined) => {
    if (!date) return
    setSelectedDate(date)
    fetchScansForDate(date)
  }

  const handleScanClick = (scan: ScanSummary) => {
    setResolvedScanId(scan.scan_id)
    setResolvedStartedAt(scan.started_at)
    setIsFallback(false)
    onScanResolved(scan.scan_id, scan.started_at)
  }

  const handleMonthChange = (month: Date) => {
    setDisplayMonth(month)
  }

  // ── Calendar day button: bold + blue for dates with scans ──────────
  const ScanDayButton = useCallback(
    (props: React.ComponentProps<typeof DayButton>) => {
      const dateStr = format(props.day.date, 'yyyy-MM-dd')
      const hasScan = scanDates.has(dateStr)

      return (
        <CalendarDayButton
          {...props}
          className={hasScan ? 'font-bold text-primary' : undefined}
        />
      )
    },
    [scanDates]
  )

  // ── Render ─────────────────────────────────────────────────────────
  return (
    <div className="border border-border rounded-lg">
      {/* Header: selected scan info + Latest button */}
      <div className="flex items-center gap-3 px-3 py-2 border-b border-border bg-muted/30">
        <div className="flex-1 min-w-0">
          {resolving ? (
            <p className="text-sm text-muted-foreground animate-pulse">Resolving...</p>
          ) : resolvedScanId && resolvedStartedAt ? (
            <p className="text-sm truncate">
              <span className="font-medium">Scan #{resolvedScanId}</span>
              <span className="text-muted-foreground">
                {' \u2014 '}
                {formatScanDate(resolvedStartedAt)} at {formatTime(resolvedStartedAt)}
              </span>
            </p>
          ) : (
            <p className="text-sm text-muted-foreground">No completed scans</p>
          )}
        </div>
        <Button
          variant="default"
          size="sm"
          onClick={jumpToLatest}
          disabled={resolving}
        >
          Latest
        </Button>
      </div>

      {/* Body: calendar (left) + scan list (right) */}
      <div className="flex">
        {/* Calendar */}
        <div className="border-r border-border shrink-0">
          <Calendar
            mode="single"
            selected={selectedDate}
            onSelect={handleDateSelect}
            month={displayMonth}
            onMonthChange={handleMonthChange}
            className="p-2 [--cell-size:1.625rem]"
            classNames={{
              month: 'flex w-full flex-col gap-2',
              week: 'mt-1 flex w-full',
            }}
            components={{
              DayButton: ScanDayButton,
            }}
          />
        </div>

        {/* Scan list */}
        <div className="flex-1 min-w-0 flex flex-col">
          <div className="px-3 py-2 border-b border-border">
            <p className="text-xs font-medium text-muted-foreground">
              {selectedDate ? `Scans on ${format(selectedDate, 'MMM d, yyyy')}` : 'Select a date'}
            </p>
          </div>

          <div className="flex-1 overflow-y-auto px-2 py-1">
            {!selectedDate ? (
              <p className="text-xs text-muted-foreground px-1 py-6 text-center">
                Click a date to see scans
              </p>
            ) : loadingDateScans ? (
              <p className="text-xs text-muted-foreground px-1 py-6 text-center animate-pulse">
                Loading...
              </p>
            ) : dateScans.length > 0 ? (
              <div className="space-y-0.5">
                {dateScans.map((scan) => (
                  <button
                    key={scan.scan_id}
                    className={cn(
                      'w-full flex items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors',
                      'hover:bg-accent',
                      scan.scan_id === resolvedScanId
                        ? 'bg-accent font-medium'
                        : ''
                    )}
                    onClick={() => handleScanClick(scan)}
                  >
                    <span className="shrink-0 w-3.5">
                      {scan.scan_id === resolvedScanId && (
                        <Check className="h-3 w-3" />
                      )}
                    </span>
                    <div className="min-w-0">
                      <div className="flex items-center gap-1.5">
                        <span className="text-xs font-medium">#{scan.scan_id}</span>
                        <span className="text-xs text-muted-foreground">
                          {formatTime(scan.started_at)}
                        </span>
                      </div>
                      <p className="text-xs text-muted-foreground truncate">
                        {formatChanges(scan.add_count, scan.modify_count, scan.delete_count)}
                      </p>
                    </div>
                  </button>
                ))}
              </div>
            ) : isFallback && resolvedScanId && resolvedStartedAt ? (
              <div className="px-1 py-4 text-xs text-muted-foreground space-y-1">
                <p>No scans on this date.</p>
                <p>
                  Showing nearest:{' '}
                  <span className="font-medium text-foreground">#{resolvedScanId}</span> from{' '}
                  {formatScanDate(resolvedStartedAt)}
                </p>
              </div>
            ) : (
              <p className="text-xs text-muted-foreground px-1 py-6 text-center">
                No scans found
              </p>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
