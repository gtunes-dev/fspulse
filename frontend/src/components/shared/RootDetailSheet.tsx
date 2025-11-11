import { useState, useEffect } from 'react'
import { Folder, Calendar, CheckCircle, XCircle, AlertTriangle } from 'lucide-react'
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { fetchQuery, countQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateFull } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'

interface RootDetailSheetProps {
  rootId: number
  rootPath: string
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface RootDetails {
  root_id: number
  root_path: string
}

interface Scan {
  scan_id: number
  started_at: number
  scan_state: string // 'C' = Completed, 'P' = Stopped, 'E' = Error
  add_count: number
  modify_count: number
  delete_count: number
  alert_count: number
  file_count: number
  folder_count: number
  total_size: number | null
}

const SCANS_PER_PAGE = 20

// Column specifications
const SCAN_COLUMNS: ColumnSpec[] = [
  { name: 'scan_id', visible: true, sort_direction: 'desc', position: 0 },
  { name: 'started_at', visible: true, sort_direction: 'none', position: 1 },
  { name: 'scan_state', visible: true, sort_direction: 'none', position: 2 },
  { name: 'add_count', visible: true, sort_direction: 'none', position: 3 },
  { name: 'modify_count', visible: true, sort_direction: 'none', position: 4 },
  { name: 'delete_count', visible: true, sort_direction: 'none', position: 5 },
  { name: 'alert_count', visible: true, sort_direction: 'none', position: 6 },
  { name: 'file_count', visible: true, sort_direction: 'none', position: 7 },
  { name: 'folder_count', visible: true, sort_direction: 'none', position: 8 },
  { name: 'total_size', visible: true, sort_direction: 'none', position: 9 },
]

// Row parsing helper
function parseScanRow(row: string[]): Scan {
  return {
    scan_id: parseInt(row[0]),
    started_at: parseInt(row[1]),
    scan_state: row[2],
    add_count: parseInt(row[3]) || 0,
    modify_count: parseInt(row[4]) || 0,
    delete_count: parseInt(row[5]) || 0,
    alert_count: parseInt(row[6]) || 0,
    file_count: parseInt(row[7]) || 0,
    folder_count: parseInt(row[8]) || 0,
    total_size: row[9] && row[9] !== '-' ? parseInt(row[9]) : null,
  }
}

export function RootDetailSheet({
  rootId,
  rootPath,
  open,
  onOpenChange,
}: RootDetailSheetProps) {
  const [loading, setLoading] = useState(false)
  const [details, setDetails] = useState<RootDetails | null>(null)
  const [scans, setScans] = useState<Scan[]>([])
  const [totalScans, setTotalScans] = useState(0)
  const [loadingMoreScans, setLoadingMoreScans] = useState(false)
  const [scheduleCount, setScheduleCount] = useState(0)

  // Extract root name from path
  const rootName = rootPath.split('/').filter(Boolean).pop() || rootPath

  useEffect(() => {
    if (!open) return

    async function loadRootDetails() {
      setLoading(true)
      try {
        // Set basic root details
        setDetails({
          root_id: rootId,
          root_path: rootPath,
        })

        // Count total scans for this root
        const scanCountResponse = await countQuery('scans', {
          columns: [{ name: 'scan_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [{ column: 'root_id', value: rootId.toString() }],
        })
        setTotalScans(scanCountResponse.count)

        // Load initial scans (most recent first)
        const scanResponse = await fetchQuery('scans', {
          columns: SCAN_COLUMNS,
          filters: [{ column: 'root_id', value: rootId.toString() }],
          limit: SCANS_PER_PAGE,
          offset: 0,
        })

        setScans(scanResponse.rows.map(parseScanRow))

        // Load schedule count
        const scheduleCountResponse = await countQuery('scan_schedules', {
          columns: [{ name: 'schedule_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [
            { column: 'root_id', value: rootId.toString() },
            { column: 'enabled', value: '1' },
          ],
        })
        setScheduleCount(scheduleCountResponse.count)
      } catch (error) {
        console.error('Error loading root details:', error)
      } finally {
        setLoading(false)
      }
    }

    loadRootDetails()
  }, [open, rootId, rootPath])

  const loadMoreScans = async () => {
    setLoadingMoreScans(true)
    try {
      const scanResponse = await fetchQuery('scans', {
        columns: SCAN_COLUMNS,
        filters: [{ column: 'root_id', value: rootId.toString() }],
        limit: SCANS_PER_PAGE,
        offset: scans.length,
      })

      const newScans = scanResponse.rows.map(parseScanRow)
      setScans([...scans, ...newScans])
    } catch (error) {
      console.error('Error loading more scans:', error)
    } finally {
      setLoadingMoreScans(false)
    }
  }

  const formatChanges = (add: number, modify: number, del: number): string => {
    const changes = []
    if (add > 0) changes.push(`${add} ${add === 1 ? 'add' : 'adds'}`)
    if (modify > 0) changes.push(`${modify} ${modify === 1 ? 'mod' : 'mods'}`)
    if (del > 0) changes.push(`${del} ${del === 1 ? 'del' : 'dels'}`)

    if (changes.length === 0) return 'No changes'
    return changes.join(', ')
  }

  const getStatusBadge = (state: string) => {
    switch (state) {
      case 'C':
        return (
          <Badge variant="success" className="gap-1">
            <CheckCircle className="h-3 w-3" />
            Completed
          </Badge>
        )
      case 'E':
        return (
          <Badge variant="destructive" className="gap-1">
            <XCircle className="h-3 w-3" />
            Error
          </Badge>
        )
      case 'P':
        return (
          <Badge className="bg-amber-500 hover:bg-amber-600 gap-1">
            <AlertTriangle className="h-3 w-3" />
            Stopped
          </Badge>
        )
      default:
        return <Badge variant="secondary">{state}</Badge>
    }
  }

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="!w-[650px] sm:!w-[700px] !max-w-[700px] overflow-y-auto">
        <SheetHeader className="space-y-4">
          <div className="flex items-start gap-4">
            <div className="flex-shrink-0">
              <Folder className="h-12 w-12 text-blue-500" />
            </div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-2xl font-bold break-words">{rootName}</SheetTitle>
              <p className="text-sm text-muted-foreground break-all mt-1">{rootPath}</p>
            </div>
          </div>
        </SheetHeader>

        {loading ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading details...</p>
          </div>
        ) : details ? (
          <div className="space-y-6 mt-6">
            {/* Current State Card */}
            <Card className="border-2">
              <CardHeader>
                <CardTitle>Current State</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Root ID</p>
                    <p className="text-base font-semibold mt-1 font-mono">{details.root_id}</p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Active Schedules</p>
                    <p className="text-base font-semibold mt-1">
                      {scheduleCount === 0 ? 'None' : scheduleCount}
                    </p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Total Scans</p>
                    <p className="text-base font-semibold mt-1">{totalScans}</p>
                  </div>
                  {scans.length > 0 && (
                    <div>
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <Calendar className="h-4 w-4" />
                        Last Scan
                      </p>
                      <p className="text-base font-semibold mt-1">
                        {formatDateFull(scans[0].started_at)}
                      </p>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            {/* Recent Scans Section */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle>Recent Scans</CardTitle>
                  {totalScans > SCANS_PER_PAGE && (
                    <p className="text-sm text-muted-foreground">
                      Showing {scans.length} of {totalScans} scan{totalScans !== 1 ? 's' : ''}
                    </p>
                  )}
                </div>
              </CardHeader>
              <CardContent className="p-6">
                {totalScans === 0 ? (
                  <div className="border border-border rounded-lg">
                    <p className="text-sm text-muted-foreground text-center py-12">
                      No scans recorded for this root
                    </p>
                  </div>
                ) : (
                  <>
                    <div className="border border-border rounded-lg">
                      <div className="p-0">
                        {scans.map((scan, idx) => (
                          <div key={scan.scan_id}>
                            <div className="p-4">
                              <div className="space-y-3">
                                {/* Header row with status and scan ID */}
                                <div className="flex items-center justify-between">
                                  <div className="flex items-center gap-2">
                                    {getStatusBadge(scan.scan_state)}
                                    <p className="text-xs text-muted-foreground">
                                      Scan <span className="font-mono font-semibold">#{scan.scan_id}</span>
                                    </p>
                                  </div>
                                  <p className="text-xs text-muted-foreground">
                                    {formatDateFull(scan.started_at)}
                                  </p>
                                </div>

                                {/* Stats grid */}
                                <div className="space-y-1 text-xs">
                                  <div className="flex items-center justify-between">
                                    <div>
                                      <span className="text-muted-foreground">Changes: </span>
                                      <span className="font-medium">{formatChanges(scan.add_count, scan.modify_count, scan.delete_count)}</span>
                                    </div>
                                    {scan.total_size !== null && (
                                      <div>
                                        <span className="text-muted-foreground">Size: </span>
                                        <span className="font-medium">{formatFileSize(scan.total_size)}</span>
                                      </div>
                                    )}
                                  </div>
                                  <div className="flex items-center justify-between">
                                    <div>
                                      <span className="text-muted-foreground">Alerts: </span>
                                      <span className={`font-medium ${scan.alert_count > 0 ? 'text-red-600' : ''}`}>
                                        {scan.alert_count}
                                      </span>
                                    </div>
                                    <div>
                                      <span className="text-muted-foreground">Items: </span>
                                      <span className="font-medium">{scan.file_count.toLocaleString()} files, {scan.folder_count.toLocaleString()} folders</span>
                                    </div>
                                  </div>
                                </div>
                              </div>
                            </div>
                            {idx < scans.length - 1 && <Separator />}
                          </div>
                        ))}
                      </div>
                    </div>
                    {totalScans > scans.length && scans.length >= SCANS_PER_PAGE && (
                      <div className="mt-4 flex justify-center">
                        <Button
                          variant="outline"
                          onClick={loadMoreScans}
                          disabled={loadingMoreScans}
                        >
                          {loadingMoreScans ? 'Loading...' : `Load ${Math.min(SCANS_PER_PAGE, totalScans - scans.length)} more`}
                        </Button>
                      </div>
                    )}
                  </>
                )}
              </CardContent>
            </Card>
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
