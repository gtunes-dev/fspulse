import { useState, useEffect, useRef, type ReactElement } from 'react'
import { Trash2 } from 'lucide-react'
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
import { CreateScheduleDialog } from './CreateScheduleDialog'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'
import { ScanDetailSheet } from '@/components/shared/ScanDetailSheet'
import { formatDateRelative } from '@/lib/dateUtils'
import { useScanManager } from '@/contexts/ScanManagerContext'
import type { RootWithScan } from '@/lib/types'

interface RootsTableProps {
  onAddRoot: () => void
  onScheduleCreated?: () => void
}

const ITEMS_PER_PAGE = 25

export function RootsTable({ onAddRoot, onScheduleCreated }: RootsTableProps) {
  const { currentScanId, lastScanCompletedAt } = useScanManager()
  const [roots, setRoots] = useState<RootWithScan[]>([])
  const [loading, setLoading] = useState(true)
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)
  const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)
  const [selectedScanId, setSelectedScanId] = useState<number | null>(null)
  const [scanSheetOpen, setScanSheetOpen] = useState(false)
  const [createScheduleDialogOpen, setCreateScheduleDialogOpen] = useState(false)
  const [preselectedRootId, setPreselectedRootId] = useState<number | undefined>(undefined)
  const [reloadTrigger, setReloadTrigger] = useState(0)
  const isInitialLoad = useRef(true)

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
  }, [lastScanCompletedAt, reloadTrigger])

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

  // Helper to format file/folder counts
  const formatCounts = (fileCount: number | null | undefined, folderCount: number | null | undefined): string => {
    if (fileCount == null || folderCount == null) return ''
    return `${fileCount.toLocaleString()} files, ${folderCount.toLocaleString()} folders`
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
                  <TableHead className="uppercase text-xs tracking-wide">Last Scan</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Schedules</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide w-32"></TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {paginatedRoots.map((root) => {
                  const scanInfo = root.last_scan
                  const scheduleCount = root.schedule_count

                  // Check if this root has the currently active scan
                  const hasActiveScan = scanInfo && currentScanId === scanInfo.scan_id

                  // Build Last Scan content
                  let lastScanContent: ReactElement
                  if (!scanInfo) {
                    lastScanContent = (
                      <span className="text-muted-foreground">Never scanned</span>
                    )
                  } else {
                    const staleness = getStalenessIndicator(scanInfo.started_at)
                    const dateText = formatDateRelative(scanInfo.started_at)
                    const dateWithStaleness = staleness ? `${dateText} ${staleness}` : dateText

                    if (scanInfo.state === 'Completed') {
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
                          <span className="text-sm text-muted-foreground">
                            {formatCounts(scanInfo.file_count, scanInfo.folder_count)}
                          </span>
                        </div>
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
                            {scanInfo.file_count != null && (
                              <span className="text-sm text-muted-foreground">
                                {scanInfo.file_count.toLocaleString()} files scanned
                              </span>
                            )}
                          </div>
                        </div>
                      )
                    } else if (['Pending', 'Scanning', 'Sweeping', 'Analyzing'].includes(scanInfo.state)) {
                      // Check if this is the currently active scan
                      const isActiveScan = currentScanId === scanInfo.scan_id

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
                                <Badge variant="default">In Progress</Badge>
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

                      {/* Last Scan Column */}
                      <TableCell>
                        {lastScanContent}
                      </TableCell>

                      {/* Schedules Column - Info only */}
                      <TableCell>
                        <span className="text-sm">
                          {scheduleCount === 0 ? (
                            <span className="text-muted-foreground">None</span>
                          ) : (
                            `${scheduleCount} ${scheduleCount === 1 ? 'schedule' : 'schedules'}`
                          )}
                        </span>
                      </TableCell>

                      {/* Add Schedule Action Column */}
                      <TableCell>
                        <Button
                          size="sm"
                          variant="default"
                          onClick={() => {
                            setPreselectedRootId(root.root_id)
                            setCreateScheduleDialogOpen(true)
                          }}
                          className="text-xs"
                        >
                          Add Schedule
                        </Button>
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
                Showing {startIndex + 1} - {endIndex} of {totalCount} roots
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
        setReloadTrigger(prev => prev + 1)
        setSelectedRoot(null)
      }}
    />

    {/* Create Schedule Dialog */}
    <CreateScheduleDialog
      open={createScheduleDialogOpen}
      onOpenChange={setCreateScheduleDialogOpen}
      preselectedRootId={preselectedRootId}
      onSuccess={() => {
        setReloadTrigger(prev => prev + 1)
        setPreselectedRootId(undefined)
        onScheduleCreated?.()
      }}
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
