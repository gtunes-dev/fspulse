import { useState, useEffect, useRef, type ReactElement } from 'react'
import { Link } from 'react-router-dom'
import { Trash2, Calendar, Play, RefreshCw, Settings2 } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DeleteRootDialog } from './DeleteRootDialog'
import { TrackingSettingsDialog } from './TrackingSettingsDialog'
import { ConfirmDeletionsDialog } from './ConfirmDeletionsDialog'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'
import { ScanDetailSheet } from '@/components/shared/ScanDetailSheet'
import { formatDateRelative, formatTimeAgo } from '@/lib/dateUtils'
import {
  describeLastCheck,
  describeTracking,
  fetchObservation,
  requestChangeCheck,
  type RootObservation,
} from '@/lib/observation'
import { formatCount } from '@/lib/formatUtils'
import { useTaskContext } from '@/contexts/useTaskContext'
import type { RootWithScan } from '@/lib/types'

interface RootsTableProps {
  onAddRoot: () => void
  onScanNow?: (rootId?: number) => void
  externalReloadTrigger?: number
}

const ITEMS_PER_PAGE = 25

export function RootsTable({ onAddRoot, onScanNow, externalReloadTrigger }: RootsTableProps) {
  const { activeRootId, currentTaskId, lastTaskCompletedAt, lastTaskScheduledAt, isPaused } = useTaskContext()
  const [roots, setRoots] = useState<RootWithScan[]>([])
  const [loading, setLoading] = useState(true)
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)
  const [selectedScanId, setSelectedScanId] = useState<number | null>(null)
  const [scanSheetOpen, setScanSheetOpen] = useState(false)
  const [internalReloadTrigger, setInternalReloadTrigger] = useState(0)
  const isInitialLoad = useRef(true)

  // How each root is being kept up to date, by root_id.
  const [observations, setObservations] = useState<Map<number, RootObservation>>(new Map())
  const [settingsRootId, setSettingsRootId] = useState<number | null>(null)
  const [confirmRootId, setConfirmRootId] = useState<number | null>(null)
  const [checkError, setCheckError] = useState<string | null>(null)

  useEffect(() => {
    async function loadRoots() {
      try {
        // Only show loading on initial mount, keep old data during refetch
        if (isInitialLoad.current) {
          setLoading(true)
          isInitialLoad.current = false
        }

        const response = await fetch('/api/roots/with-scans')
        if (!response.ok) throw new Error('Failed to load roots')

        const data: RootWithScan[] = await response.json()
        setRoots(data)
        setTotalCount(data.length)
      } catch (error) {
        console.error('Error loading roots:', error)
      } finally {
        setLoading(false)
      }
    }

    loadRoots()
  }, [activeRootId, currentTaskId, lastTaskCompletedAt, lastTaskScheduledAt, internalReloadTrigger, externalReloadTrigger])

  // Change tracking status. Reloaded with the roots, and on a timer:
  // the watch state changes without any task starting or finishing.
  useEffect(() => {
    let cancelled = false
    const load = () => {
      fetchObservation()
        .then((data) => {
          if (!cancelled) setObservations(new Map(data.map((o) => [o.root_id, o])))
        })
        .catch((error) => console.error('Error loading change tracking status:', error))
    }
    load()
    const timer = window.setInterval(load, 30_000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [activeRootId, currentTaskId, lastTaskCompletedAt, lastTaskScheduledAt, internalReloadTrigger, externalReloadTrigger])

  const startChangeCheck = async (rootId: number) => {
    setCheckError(null)
    try {
      await requestChangeCheck(rootId)
      setInternalReloadTrigger((prev) => prev + 1)
    } catch (error) {
      setCheckError(error instanceof Error ? error.message : 'Failed to start the check')
    }
  }

  // Pagination
  const startIndex = (currentPage - 1) * ITEMS_PER_PAGE
  const endIndex = Math.min(startIndex + ITEMS_PER_PAGE, totalCount)
  const paginatedRoots = roots.slice(startIndex, endIndex)

  // Helper function to get staleness indicator based on scan age
  // Option A: 0-14 days (none), 14-30 (> 2 weeks), 30-365 (> 1 month), > 365 (> 1 year)
  const getStalenessIndicator = (timestamp: number): string | null => {
    const now = Date.now() / 1000  // Convert to seconds
    const ageInSeconds = now - timestamp
    const ageInDays = ageInSeconds / 86400

    if (ageInDays < 14) return null
    if (ageInDays < 30) return '(> 2 weeks)'
    if (ageInDays < 365) return '(> 1 month)'
    return '(> 1 year)'
  }


  if (loading) {
    return (
      <Card>
        <CardContent className="py-8">
          <div className="text-center text-muted-foreground">Loading roots...</div>
        </CardContent>
      </Card>
    )
  }

  return (
    <>
    <Card>
      <CardHeader>
        <CardTitle>Roots</CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Action Bar */}
        <div className="flex items-center">
          <Button onClick={onAddRoot} size="default">
            Add Root
          </Button>
        </div>

        {checkError && (
          <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
            {checkError}
          </div>
        )}

        {paginatedRoots.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            No roots found. Click "Add Root" to get started.
          </div>
        ) : (
          <>
            {/* Bordered Table Container */}
            <div className="border border-border rounded-lg overflow-hidden">
              <Table>
                <TableHeader className="bg-muted">
                <TableRow>
                  <TableHead className="w-10"></TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Change Tracking</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Last Integrity Check</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Schedules</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide w-40"></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {paginatedRoots.map((root) => {
                  const scanInfo = root.last_scan
                  const scheduleCount = root.schedule_count

                  // Check if this root has the currently active scan
                  const hasActiveScan = activeRootId === root.root_id

                  // Change tracking: the watch state, when the whole root was
                  // last compared, and anything being held back. Every row
                  // shows the same three things.
                  const observation = observations.get(root.root_id)
                  let trackingContent: ReactElement
                  if (!observation) {
                    trackingContent = <span className="text-muted-foreground">Loading...</span>
                  } else {
                    const tracking = describeTracking(observation)
                    trackingContent = (
                      <div className="flex flex-col gap-1">
                        <div className="flex items-center gap-2" title={tracking.explanation ?? undefined}>
                          {tracking.attention ? (
                            <Badge variant={tracking.attention}>{tracking.label}</Badge>
                          ) : (
                            <span>{tracking.label}</span>
                          )}
                          <Button
                            size="sm"
                            variant="ghost"
                            className="h-6 w-6 p-0 text-muted-foreground"
                            title="Change tracking settings"
                            aria-label="Change tracking settings"
                            onClick={() => setSettingsRootId(root.root_id)}
                          >
                            <Settings2 className="h-3.5 w-3.5" />
                          </Button>
                        </div>
                        {tracking.explanation && (
                          <span className="text-xs text-muted-foreground max-w-xs">{tracking.explanation}</span>
                        )}
                        <div className="flex items-center gap-1 text-xs text-muted-foreground">
                          <span>
                            {describeLastCheck(
                              observation,
                              hasActiveScan && !observation.last_walk_completed_at,
                              formatTimeAgo
                            )}
                          </span>
                          <Button
                            size="sm"
                            variant="ghost"
                            className="h-5 w-5 p-0"
                            title="Check the whole root for changes now"
                            aria-label="Check the whole root for changes now"
                            disabled={hasActiveScan || observation.watch === 'offline'}
                            onClick={() => startChangeCheck(root.root_id)}
                          >
                            <RefreshCw className="h-3 w-3" />
                          </Button>
                        </div>
                        {observation.ignore_pending && (
                          <span className="text-xs text-muted-foreground max-w-xs">
                            Ignore changes take full effect at the next full check.
                          </span>
                        )}
                        {observation.walk_block && (
                          <div className="flex flex-col items-start gap-1">
                            <div className="flex items-center gap-2">
                              <Badge variant="warning">Deletions not recorded</Badge>
                            </div>
                            <span className="text-xs text-muted-foreground max-w-xs">
                              {observation.walk_block === 'empty_root'
                                ? 'The folder is empty. If its drive is disconnected, reconnect it.'
                                : 'Most of this root is missing.'}
                            </span>
                            <Button
                              size="sm"
                              variant="outline"
                              className="h-6 text-xs"
                              onClick={() => setConfirmRootId(root.root_id)}
                            >
                              Review
                            </Button>
                          </div>
                        )}
                      </div>
                    )
                  }

                  // Build Last Integrity Check content
                  let lastScanContent: ReactElement
                  if (!scanInfo) {
                    lastScanContent = (
                      <span className="text-muted-foreground">Never checked</span>
                    )
                  } else {
                    const staleness = getStalenessIndicator(scanInfo.started_at)
                    const dateText = formatDateRelative(scanInfo.started_at)
                    const dateWithStaleness = staleness ? `${dateText} ${staleness}` : dateText

                    if (scanInfo.state === 'Completed') {
                      lastScanContent = (
                        <button
                          className="text-left hover:underline"
                          onClick={() => {
                            setSelectedScanId(scanInfo.scan_id)
                            setScanSheetOpen(true)
                          }}
                        >
                          <span>{dateWithStaleness}</span>
                        </button>
                      )
                    } else if (scanInfo.state === 'Error') {
                      lastScanContent = (
                        <div className="flex flex-col gap-0.5">
                          <button
                            className="text-left hover:underline"
                            onClick={() => {
                              setSelectedScanId(scanInfo.scan_id)
                              setScanSheetOpen(true)
                            }}
                          >
                            <span>{dateWithStaleness}</span>
                          </button>
                          <div className="flex items-center gap-2">
                            <Badge variant="error">Error</Badge>
                            <span className="text-sm text-muted-foreground">
                              {scanInfo.error || 'Unknown error'}
                            </span>
                          </div>
                        </div>
                      )
                    } else if (scanInfo.state === 'Stopped') {
                      lastScanContent = (
                        <div className="flex flex-col gap-0.5">
                          <button
                            className="text-left hover:underline"
                            onClick={() => {
                              setSelectedScanId(scanInfo.scan_id)
                              setScanSheetOpen(true)
                            }}
                          >
                            <span>{dateWithStaleness}</span>
                          </button>
                          <div className="flex items-center gap-2">
                            <Badge variant="warning">Stopped</Badge>
                          </div>
                        </div>
                      )
                    } else if (['Pending', 'Scanning'].includes(scanInfo.state)) {
                      // Check if this is the currently active scan
                      const isActiveScan = activeRootId === root.root_id

                      lastScanContent = (
                        <div className="flex flex-col gap-0.5">
                          <button
                            className="text-left hover:underline"
                            onClick={() => {
                              setSelectedScanId(scanInfo.scan_id)
                              setScanSheetOpen(true)
                            }}
                          >
                            <span>{dateWithStaleness}</span>
                          </button>
                          <div className="flex items-center gap-2">
                            {isActiveScan ? (
                              <>
                                <Badge variant="success">In Progress</Badge>
                                <span className="text-sm text-muted-foreground">
                                  {scanInfo.state} phase
                                </span>
                              </>
                            ) : isPaused ? (
                              <>
                                <Badge variant="info-alternate">Paused</Badge>
                                <span className="text-sm text-muted-foreground">
                                  {scanInfo.state} phase
                                </span>
                              </>
                            ) : (
                              <>
                                <Badge variant="warning">Incomplete</Badge>
                                <span className="text-sm text-muted-foreground">
                                  {scanInfo.state} phase
                                </span>
                              </>
                            )}
                          </div>
                        </div>
                      )
                    } else {
                      // Fallback for unknown states
                      lastScanContent = (
                        <button
                          className="text-left hover:underline"
                          onClick={() => {
                            setSelectedScanId(scanInfo.scan_id)
                            setScanSheetOpen(true)
                          }}
                        >
                          <span>{dateWithStaleness}</span>
                        </button>
                      )
                    }
                  }

                  return (
                    <TableRow key={root.root_id}>
                      {/* Delete Icon Column */}
                      <TableCell className="w-10 pr-2">
                        <Button
                          size="sm"
                          variant="ghost"
                          disabled={hasActiveScan}
                          onClick={() => {
                            setSelectedRoot({ id: root.root_id, path: root.root_path })
                            setDeleteDialogOpen(true)
                          }}
                          className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                        >
                          <Trash2 className="h-5 w-5" />
                        </Button>
                      </TableCell>

                      {/* Root Path Column */}
                      <TableCell>
                        <button
                          className="font-medium text-left hover:underline hover:text-primary"
                          onClick={() => {
                            setSelectedRoot({ id: root.root_id, path: root.root_path })
                            setRootSheetOpen(true)
                          }}
                        >
                          {root.root_path}
                        </button>
                      </TableCell>

                      {/* Change Tracking Column */}
                      <TableCell className="align-top">
                        {trackingContent}
                      </TableCell>

                      {/* Last Integrity Check Column */}
                      <TableCell>
                        {lastScanContent}
                      </TableCell>

                      {/* Schedules Column - links to Schedules page */}
                      <TableCell>
                        <Link
                          to={`/schedules?root_id=${root.root_id}`}
                          className="flex items-center gap-1.5 text-sm hover:text-primary transition-colors"
                          title="View schedules"
                        >
                          <Calendar className="h-4 w-4 text-muted-foreground" />
                          {scheduleCount === 0 ? (
                            <span className="text-muted-foreground">None</span>
                          ) : (
                            <span>{formatCount(scheduleCount)} {scheduleCount === 1 ? 'schedule' : 'schedules'}</span>
                          )}
                        </Link>
                      </TableCell>

                      {/* Scan Now Column */}
                      <TableCell>
                        {onScanNow && (
                          <Button
                            size="sm"
                            variant="default"
                            disabled={hasActiveScan}
                            onClick={() => onScanNow(root.root_id)}
                            className="text-xs"
                          >
                            <Play className="h-3.5 w-3.5 mr-1.5" />
                            Check Integrity
                          </Button>
                        )}
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>
          </div>

          {/* Pagination */}
          {totalCount > ITEMS_PER_PAGE && (
            <div className="flex items-center justify-between pt-4">
              <div className="text-sm text-muted-foreground">
                Showing {(startIndex + 1).toLocaleString()} - {endIndex.toLocaleString()} of {totalCount.toLocaleString()} roots
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => currentPage > 1 && setCurrentPage(p => p - 1)}
                  disabled={currentPage === 1}
                >
                  Previous
                </Button>
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => endIndex < totalCount && setCurrentPage(p => p + 1)}
                  disabled={endIndex >= totalCount}
                >
                  Next
                </Button>
              </div>
            </div>
          )}
        </>
      )}
      </CardContent>
    </Card>

    {/* Delete Root Dialog */}
    <DeleteRootDialog
      open={deleteDialogOpen}
      onOpenChange={setDeleteDialogOpen}
      rootId={selectedRoot?.id ?? null}
      rootPath={selectedRoot?.path ?? ''}
      onDeleteSuccess={() => {
        setInternalReloadTrigger(prev => prev + 1)
        setSelectedRoot(null)
      }}
    />

    {/* Change tracking settings */}
    <TrackingSettingsDialog
      observation={settingsRootId !== null ? observations.get(settingsRootId) ?? null : null}
      open={settingsRootId !== null}
      onOpenChange={(open) => !open && setSettingsRootId(null)}
      onSaved={() => setInternalReloadTrigger((prev) => prev + 1)}
    />

    {/* Confirm held-back deletions */}
    <ConfirmDeletionsDialog
      observation={confirmRootId !== null ? observations.get(confirmRootId) ?? null : null}
      open={confirmRootId !== null}
      onOpenChange={(open) => !open && setConfirmRootId(null)}
      onConfirmed={() => setInternalReloadTrigger((prev) => prev + 1)}
    />

    {/* Root Detail Sheet */}
    {selectedRoot && (
      <RootDetailSheet
        rootId={selectedRoot.id}
        rootPath={selectedRoot.path}
        open={rootSheetOpen}
        onOpenChange={setRootSheetOpen}
      />
    )}

    {/* Scan Detail Sheet */}
    {selectedScanId && (
      <ScanDetailSheet
        scanId={selectedScanId}
        open={scanSheetOpen}
        onOpenChange={setScanSheetOpen}
      />
    )}
  </>
  )
}
