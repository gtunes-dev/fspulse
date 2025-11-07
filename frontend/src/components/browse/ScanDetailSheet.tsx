import { useState, useEffect } from 'react'
import { Activity, Calendar, CheckCircle, XCircle, AlertTriangle } from 'lucide-react'
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateFull } from '@/lib/dateUtils'

interface ScanDetailSheetProps {
  scanId: number
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface ScanDetails {
  scan_id: number
  root_id: number
  root_path: string
  scan_time: number
  scan_state: string // 'C' = Completed, 'P' = Stopped, 'E' = Error, etc.
  add_count: number
  modify_count: number
  delete_count: number
  alert_count: number
  file_count: number
  folder_count: number
  total_file_size: number | null
}

// Column specifications for scan query (matching RootDetailSheet)
const SCAN_COLUMNS: ColumnSpec[] = [
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
  { name: 'root_id', visible: true, sort_direction: 'none', position: 1 },
  { name: 'scan_time', visible: true, sort_direction: 'none', position: 2 },
  { name: 'scan_state', visible: true, sort_direction: 'none', position: 3 },
  { name: 'add_count', visible: true, sort_direction: 'none', position: 4 },
  { name: 'modify_count', visible: true, sort_direction: 'none', position: 5 },
  { name: 'delete_count', visible: true, sort_direction: 'none', position: 6 },
  { name: 'alert_count', visible: true, sort_direction: 'none', position: 7 },
  { name: 'file_count', visible: true, sort_direction: 'none', position: 8 },
  { name: 'folder_count', visible: true, sort_direction: 'none', position: 9 },
  { name: 'total_file_size', visible: true, sort_direction: 'none', position: 10 },
]

// Column specifications for root query
const ROOT_COLUMNS: ColumnSpec[] = [
  { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
  { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
]

export function ScanDetailSheet({
  scanId,
  open,
  onOpenChange,
}: ScanDetailSheetProps) {
  const [loading, setLoading] = useState(false)
  const [details, setDetails] = useState<ScanDetails | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return

    async function loadScanDetails() {
      setLoading(true)
      setError(null)
      try {
        // Load scan details
        const scanResponse = await fetchQuery('scans', {
          columns: SCAN_COLUMNS,
          filters: [{ column: 'scan_id', value: scanId.toString() }],
          limit: 1,
        })

        if (scanResponse.rows.length === 0) {
          console.error('Scan not found:', scanId)
          setError(`Scan #${scanId} not found`)
          return
        }

        console.log('Scan data loaded:', scanResponse.rows[0])

        const row = scanResponse.rows[0]
        const rootId = parseInt(row[1])

        // Load root path
        const rootResponse = await fetchQuery('roots', {
          columns: ROOT_COLUMNS,
          filters: [{ column: 'root_id', value: rootId.toString() }],
          limit: 1,
        })

        const rootPath = rootResponse.rows.length > 0 ? rootResponse.rows[0][1] : `Root ${rootId}`

        setDetails({
          scan_id: parseInt(row[0]),
          root_id: rootId,
          root_path: rootPath,
          scan_time: parseInt(row[2]),
          scan_state: row[3],
          add_count: parseInt(row[4]) || 0,
          modify_count: parseInt(row[5]) || 0,
          delete_count: parseInt(row[6]) || 0,
          alert_count: parseInt(row[7]) || 0,
          file_count: parseInt(row[8]) || 0,
          folder_count: parseInt(row[9]) || 0,
          total_file_size: row[10] && row[10] !== '-' ? parseInt(row[10]) : null,
        })
      } catch (error) {
        console.error('Error loading scan details:', error)
        setError(error instanceof Error ? error.message : 'Failed to load scan details')
      } finally {
        setLoading(false)
      }
    }

    loadScanDetails()
  }, [open, scanId])

  const formatFileSize = (bytes: number | null): string => {
    if (bytes === null) return 'N/A'
    if (bytes === 0) return '0 B'
    const units = ['B', 'KB', 'MB', 'GB', 'TB']
    let size = bytes
    let unitIndex = 0
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024
      unitIndex++
    }
    return `${size.toFixed(2)} ${units[unitIndex]}`
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
              <Activity className="h-12 w-12 text-blue-500" />
            </div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-2xl font-bold">Scan #{scanId}</SheetTitle>
              {details && (
                <p className="text-sm text-muted-foreground break-all mt-1">{details.root_path}</p>
              )}
            </div>
          </div>
        </SheetHeader>

        {loading ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading details...</p>
          </div>
        ) : error ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-red-500">{error}</p>
          </div>
        ) : details ? (
          <div className="space-y-6 mt-6">
            {/* Current State Card */}
            <Card className="border-2">
              <CardHeader>
                <CardTitle>Scan Details</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="space-y-4">
                  {/* Status and Time */}
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Status</p>
                      <div className="mt-1">
                        {getStatusBadge(details.scan_state)}
                      </div>
                    </div>
                    <div>
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <Calendar className="h-4 w-4" />
                        Scan Time
                      </p>
                      <p className="text-base font-semibold mt-1">
                        {formatDateFull(details.scan_time)}
                      </p>
                    </div>
                  </div>

                  {/* Root ID */}
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Root ID</p>
                    <p className="text-base font-semibold mt-1 font-mono">{details.root_id}</p>
                  </div>

                  {/* Changes and Size */}
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Changes</p>
                      <p className="text-base font-semibold mt-1">
                        {formatChanges(details.add_count, details.modify_count, details.delete_count)}
                      </p>
                    </div>
                    {details.total_file_size !== null && (
                      <div className="text-right">
                        <p className="text-sm font-medium text-muted-foreground">Total Size</p>
                        <p className="text-base font-semibold mt-1">
                          {formatFileSize(details.total_file_size)}
                        </p>
                      </div>
                    )}
                  </div>

                  {/* Alerts and Items */}
                  <div className="flex items-center justify-between">
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Alerts</p>
                      <p className={`text-base font-semibold mt-1 ${details.alert_count > 0 ? 'text-red-600' : ''}`}>
                        {details.alert_count}
                      </p>
                    </div>
                    <div className="text-right">
                      <p className="text-sm font-medium text-muted-foreground">Items Scanned</p>
                      <p className="text-base font-semibold mt-1">
                        {details.file_count.toLocaleString()} files, {details.folder_count.toLocaleString()} folders
                      </p>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
