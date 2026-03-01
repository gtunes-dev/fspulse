import { useState, useEffect, useRef } from 'react'
import { Link } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatDateTimeShort } from '@/lib/dateUtils'
import { useTaskContext } from '@/contexts/TaskContext'
import { CheckCircle, XCircle, AlertTriangle, Calendar, Play } from 'lucide-react'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'
import { RootFilter } from '@/components/shared/RootFilter'
import { fetchQuery } from '@/lib/api'

interface ScanHistoryRow {
  scan_id: number
  root_id: number
  started_at: number
  ended_at: number | null
  was_restarted: boolean
  schedule_id: number | null
  schedule_name: string | null
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  state: number
}

interface Root {
  id: number
  path: string
}

interface RootMap {
  [root_id: number]: string
}

const ITEMS_PER_PAGE = 25

export function ScanHistoryTable() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [scans, setScans] = useState<ScanHistoryRow[]>([])
  const [roots, setRoots] = useState<Root[]>([])
  const [rootMap, setRootMap] = useState<RootMap>({})
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)
  const [selectedRootId, setSelectedRootId] = useState<string>('all')
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)
  const isInitialLoad = useRef(true)
  const lastFilterKeyRef = useRef<string>('')

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
        const response = await fetchQuery('roots', {
          columns: [
            { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
            { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
          ],
          filters: [],
          limit: 1000,
        })

        // Create root_id -> root_path map
        const map: RootMap = {}
        const rootsList: Root[] = []
        response.rows.forEach((row) => {
          const rootId = parseInt(row[0])
          const rootPath = row[1]
          map[rootId] = rootPath
          rootsList.push({ id: rootId, path: rootPath })
        })
        setRootMap(map)
        setRoots(rootsList)
      } catch (err) {
        console.error('Error loading roots:', err)
      }
    }

    loadRoots()
  }, [])

  // Load scan history
  useEffect(() => {
    async function loadScanHistory() {
      try {
        // Only show loading on initial mount, keep old data during refetch
        if (isInitialLoad.current) {
          setLoading(true)
          isInitialLoad.current = false
        }
        setError(null)

        // Build filter key to detect when filters change
        const root_id = selectedRootId === 'all' ? undefined : parseInt(selectedRootId)
        const filterKey = `${root_id || 'all'}`
        const needsCount = filterKey !== lastFilterKeyRef.current

        // Get count only when filters change
        if (needsCount) {
          const countParams = new URLSearchParams()
          if (root_id) {
            countParams.append('root_id', root_id.toString())
          }

          const countResponse = await fetch(`/api/scans/scan_history/count?${countParams}`)
          if (!countResponse.ok) {
            throw new Error(`Count query failed: ${countResponse.statusText}`)
          }

          const countData = await countResponse.json()
          setTotalCount(countData.count)
          lastFilterKeyRef.current = filterKey
        }

        // Always fetch current page
        const fetchParams = new URLSearchParams({
          limit: ITEMS_PER_PAGE.toString(),
          offset: ((currentPage - 1) * ITEMS_PER_PAGE).toString(),
        })
        if (root_id) {
          fetchParams.append('root_id', root_id.toString())
        }

        const fetchResponse = await fetch(`/api/scans/scan_history/fetch?${fetchParams}`)
        if (!fetchResponse.ok) {
          throw new Error(`Fetch query failed: ${fetchResponse.statusText}`)
        }

        const fetchData = await fetchResponse.json()
        setScans(fetchData.scans)
      } catch (err) {
        console.error('Error loading scan history:', err)
        setError(err instanceof Error ? err.message : 'Failed to load scan history')
      } finally {
        setLoading(false)
      }
    }

    loadScanHistory()
  }, [lastTaskCompletedAt, selectedRootId, currentPage])

  // Reset to page 1 when filters change
  useEffect(() => {
    setCurrentPage(1)
  }, [selectedRootId])

  // Format changes
  const formatChanges = (add: number | null, modify: number | null, del: number | null): string => {
    const changes = []
    if (add && add > 0) changes.push(`${add} ${add === 1 ? 'add' : 'adds'}`)
    if (modify && modify > 0) changes.push(`${modify} ${modify === 1 ? 'mod' : 'mods'}`)
    if (del && del > 0) changes.push(`${del} ${del === 1 ? 'del' : 'dels'}`)

    if (changes.length === 0) return 'No changes'
    return changes.join(', ')
  }

  // Format duration
  const formatDuration = (scan: ScanHistoryRow): string => {
    // Case 1: was_restarted is true - show "-"
    if (scan.was_restarted) {
      return '-'
    }

    // Case 2: has started_at but not ended_at - show "-"
    if (!scan.ended_at) {
      return '-'
    }

    // Case 3: has both started_at and ended_at - calculate and show duration
    const durationSeconds = scan.ended_at - scan.started_at

    // Format duration nicely
    const hours = Math.floor(durationSeconds / 3600)
    const minutes = Math.floor((durationSeconds % 3600) / 60)
    const seconds = durationSeconds % 60

    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`
    } else if (minutes > 0) {
      return `${minutes}m ${seconds}s`
    } else {
      return `${seconds}s`
    }
  }

  // Get status icon
  const getStatusIcon = (state: number) => {
    switch (state) {
      case 4: // Completed
        return <CheckCircle className="h-4 w-4 text-green-500" />
      case 6: // Error
        return <XCircle className="h-4 w-4 text-red-500" />
      case 5: // Stopped
        return <AlertTriangle className="h-4 w-4 text-orange-500" />
      default:
        return null
    }
  }

  // Get status text
  const getStatusText = (state: number) => {
    switch (state) {
      case 4:
        return 'Completed'
      case 6:
        return 'Error'
      case 5:
        return 'Stopped'
      default:
        return `State ${state}`
    }
  }

  // Shorten path
  const shortenPath = (path: string, maxLength: number = 30): string => {
    if (path.length <= maxLength) return path
    const parts = path.split('/')
    if (parts.length <= 2) return path

    // Show first and last parts
    return `${parts[0]}/.../${parts[parts.length - 1]}`
  }

  // Pagination
  const start = (currentPage - 1) * ITEMS_PER_PAGE + 1
  const end = Math.min(start + ITEMS_PER_PAGE - 1, totalCount)

  if (loading && scans.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Scan History</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-muted-foreground text-center py-4">
            Loading scan history...
          </p>
        </CardContent>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Scan History</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-red-500 text-center py-4">
            Error: {error}
          </p>
        </CardContent>
      </Card>
    )
  }

  if (scans.length === 0 && !loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Scan History</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Root Filter */}
          <RootFilter
            roots={roots}
            selectedRootId={selectedRootId}
            onRootChange={setSelectedRootId}
          />

          <div className="border border-border rounded-lg">
            <p className="text-sm text-muted-foreground text-center py-12">
              {selectedRootId === 'all'
                ? 'No completed scans yet'
                : 'No completed scans for this root'}
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
          <CardTitle>Scan History</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Root Filter */}
          <RootFilter
            roots={roots}
            selectedRootId={selectedRootId}
            onRootChange={setSelectedRootId}
          />

          {/* Bordered Table */}
          <div className="border border-border rounded-lg overflow-hidden">
            <Table>
              <TableHeader className="bg-muted">
                <TableRow>
                  <TableHead className="uppercase text-xs tracking-wide">Started</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Scan</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Schedule</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide w-[120px]">Duration</TableHead>
                  <TableHead className="text-center uppercase text-xs tracking-wide">Changes</TableHead>
                  <TableHead className="text-right uppercase text-xs tracking-wide">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {scans.map((scan) => (
                  <TableRow key={scan.scan_id}>
                    <TableCell className="font-medium">
                      {formatDateTimeShort(scan.started_at)}
                    </TableCell>
                    <TableCell>
                      <Link
                        to={`/browse?root_id=${scan.root_id}&scan_id=${scan.scan_id}`}
                        className="hover:underline hover:text-primary"
                      >
                        Scan #{scan.scan_id}
                      </Link>
                    </TableCell>
                    <TableCell
                      className="max-w-[200px] truncate"
                      title={rootMap[scan.root_id] || `Root ${scan.root_id}`}
                    >
                      <button
                        onClick={(e) => {
                          e.stopPropagation()
                          setSelectedRoot({
                            id: scan.root_id,
                            path: rootMap[scan.root_id] || `Root ${scan.root_id}`
                          })
                          setRootSheetOpen(true)
                        }}
                        className="text-left hover:underline hover:text-primary cursor-pointer"
                      >
                        {shortenPath(rootMap[scan.root_id] || `Root ${scan.root_id}`)}
                      </button>
                    </TableCell>
                    <TableCell>
                      {scan.schedule_name ? (
                        <div className="flex items-center gap-2">
                          <Calendar className="h-4 w-4 text-blue-500" />
                          <span>{scan.schedule_name}</span>
                        </div>
                      ) : (
                        <div className="flex items-center gap-2">
                          <Play className="h-4 w-4 text-blue-500" />
                          <span>Manual Scan</span>
                        </div>
                      )}
                    </TableCell>
                    <TableCell>
                      {formatDuration(scan)}
                    </TableCell>
                    <TableCell className="text-center">
                      {formatChanges(scan.add_count, scan.modify_count, scan.delete_count)}
                    </TableCell>
                    <TableCell className="text-right">
                      <div className="flex items-center justify-end gap-2">
                        {getStatusIcon(scan.state)}
                        <span className="text-sm">{getStatusText(scan.state)}</span>
                      </div>
                    </TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>

          {/* Pagination */}
          {totalCount > ITEMS_PER_PAGE && (
            <div className="flex items-center justify-between">
              <div className="text-sm text-muted-foreground whitespace-nowrap">
                Showing {(totalCount > 0 ? start : 0).toLocaleString()} - {end.toLocaleString()} of {totalCount.toLocaleString()} scans
              </div>
              <div className="flex gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setCurrentPage((prev) => prev - 1)}
                  disabled={currentPage === 1 || loading}
                >
                  Previous
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setCurrentPage((prev) => prev + 1)}
                  disabled={end >= totalCount || loading}
                >
                  Next
                </Button>
              </div>
            </div>
          )}
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
