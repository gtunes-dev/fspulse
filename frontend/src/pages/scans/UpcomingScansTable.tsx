import { useState, useEffect, useRef } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { useTaskContext } from '@/contexts/TaskContext'
import { Clock, Calendar, CirclePause } from 'lucide-react'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'

interface UpcomingScan {
  queue_id: number
  root_id: number
  root_path: string
  schedule_id: number | null
  schedule_name: string | null
  next_scan_time: number  // Unix timestamp
  source: string  // "Manual" or "Scheduled"
  is_queued: boolean  // true if waiting to start (next_scan_time <= now)
  scan_id: number | null  // Non-null if this is an in-progress scan that is paused
}

export function UpcomingScansTable() {
  const { currentTaskId, lastTaskCompletedAt, lastTaskScheduledAt, isPaused } = useTaskContext()
  const [scans, setScans] = useState<UpcomingScan[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)
  const isInitialLoad = useRef(true)

  useEffect(() => {
    async function loadData() {
      try {
        // Only show loading on initial mount, keep old data during refetch
        if (isInitialLoad.current) {
          setLoading(true)
          isInitialLoad.current = false
        }
        setError(null)

        const response = await fetch('/api/schedules/upcoming')
        if (!response.ok) {
          throw new Error(`Failed to fetch upcoming scans: ${response.statusText}`)
        }

        const data = await response.json()
        setScans(data.upcoming_scans || [])
      } catch (err) {
        console.error('Error loading upcoming scans:', err)
        setError(err instanceof Error ? err.message : 'Failed to load upcoming scans')
      } finally {
        setLoading(false)
      }
    }

    loadData()
  }, [currentTaskId, lastTaskCompletedAt, lastTaskScheduledAt])

  const formatNextRun = (timestamp: number, isQueued: boolean, queuePosition: number): string => {
    // For queued scans, show queue position instead of time
    if (isQueued) {
      if (queuePosition === 0) return 'Next'
      const position = queuePosition + 1
      const suffix = position === 2 ? 'nd' : position === 3 ? 'rd' : 'th'
      return `${position}${suffix} in queue`
    }

    // For scheduled scans, show relative time
    const now = Date.now() / 1000  // Convert to seconds
    const diff = timestamp - now

    const minutes = Math.floor(diff / 60)
    const hours = Math.floor(diff / 3600)
    const days = Math.floor(diff / 86400)

    if (minutes < 60) return `in ${minutes} min`
    if (hours < 24) return `in ${hours} ${hours === 1 ? 'hour' : 'hours'}`
    return `in ${days} ${days === 1 ? 'day' : 'days'}`
  }

  const shortenPath = (path: string, maxLength: number = 30): string => {
    if (path.length <= maxLength) return path
    const parts = path.split('/')
    if (parts.length <= 2) return path

    // Show first and last parts
    return `${parts[0]}/.../${parts[parts.length - 1]}`
  }

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Upcoming Scans</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-muted-foreground text-center py-4">
            Loading upcoming scans...
          </p>
        </CardContent>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Upcoming Scans</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-red-500 text-center py-4">
            Error: {error}
          </p>
        </CardContent>
      </Card>
    )
  }

  if (scans.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Upcoming Scans</CardTitle>
        </CardHeader>
        <CardContent className="p-6">
          <div className="border border-border rounded-lg">
            <p className="text-sm text-muted-foreground text-center py-12">
              No upcoming scans scheduled
            </p>
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Upcoming Scans</CardTitle>
        </CardHeader>
        <CardContent className="p-6">
          <div className="border border-border rounded-lg overflow-hidden">
            <Table>
              <TableHeader className="bg-muted">
                <TableRow>
                  <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Schedule</TableHead>
                  <TableHead className="text-center uppercase text-xs tracking-wide">Status</TableHead>
                  <TableHead className="text-right uppercase text-xs tracking-wide">Next Run</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {scans.map((scan) => {
                  // Calculate queue position for queued scans
                  const queuedScans = scans.filter(s => s.is_queued)
                  const queuePosition = queuedScans.findIndex(s => s.queue_id === scan.queue_id)

                  return (
                    <TableRow key={scan.queue_id}>
                      <TableCell
                        className="max-w-[200px] truncate"
                        title={scan.root_path}
                      >
                        <button
                          onClick={() => {
                            setSelectedRoot({ id: scan.root_id, path: scan.root_path })
                            setRootSheetOpen(true)
                          }}
                          className="text-left hover:underline hover:text-primary cursor-pointer"
                        >
                          {shortenPath(scan.root_path)}
                        </button>
                      </TableCell>
                      <TableCell>
                        {scan.schedule_name || <span className="text-muted-foreground">(Manual)</span>}
                      </TableCell>
                      <TableCell className="text-center">
                        <div className="flex items-center justify-center gap-2">
                          {scan.scan_id !== null ? (
                            <>
                              <CirclePause className="h-4 w-4 text-purple-500" />
                              <span className="text-sm">Paused</span>
                            </>
                          ) : scan.is_queued ? (
                            <>
                              <Clock className="h-4 w-4 text-purple-500" />
                              <span className="text-sm">Queued</span>
                            </>
                          ) : (
                            <>
                              <Calendar className="h-4 w-4 text-blue-500" />
                              <span className="text-sm">Scheduled</span>
                            </>
                          )}
                        </div>
                      </TableCell>
                      <TableCell className="text-right font-medium">
                        {scan.scan_id !== null
                          ? 'When unpaused'
                          : (scan.is_queued && queuePosition === 0 && isPaused)
                            ? 'When unpaused'
                            : formatNextRun(scan.next_scan_time, scan.is_queued, queuePosition)
                        }
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          </div>
        </CardContent>
      </Card>

      {/* Root Detail Sheet */}
      {selectedRoot && (
        <RootDetailSheet
          rootId={selectedRoot.id}
          rootPath={selectedRoot.path}
          open={rootSheetOpen}
          onOpenChange={setRootSheetOpen}
        />
      )}
    </>
  )
}
