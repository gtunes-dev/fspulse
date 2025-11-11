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
import { fetchQuery } from '@/lib/api'
import { formatTimeAgo } from '@/lib/dateUtils'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { CheckCircle, XCircle, AlertTriangle } from 'lucide-react'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'

interface ScanRow {
  scan_id: number
  root_id: number
  scan_time: number // Unix timestamp
  add_count: number
  modify_count: number
  delete_count: number
  scan_state: string // 'C' = Completed, 'P' = Stopped, 'E' = Error
}

interface RootMap {
  [root_id: number]: string
}

export function RecentScansTable() {
  const { lastScanCompletedAt } = useScanManager()
  const [scans, setScans] = useState<ScanRow[]>([])
  const [roots, setRoots] = useState<RootMap>({})
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

        // Load roots first to create a map
        const rootsResponse = await fetchQuery('roots', {
          columns: [
            { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
            { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
          ],
          filters: [],
          limit: 1000, // Get all roots (small dataset)
        })

        // Create root_id -> root_path map
        const rootMap: RootMap = {}
        rootsResponse.rows.forEach((row) => {
          const rootId = parseInt(row[0])
          const rootPath = row[1]
          rootMap[rootId] = rootPath
        })
        setRoots(rootMap)

        // Load recent scans (completed, stopped, or error states only)
        const scansResponse = await fetchQuery('scans', {
          columns: [
            { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
            { name: 'root_id', visible: true, sort_direction: 'none', position: 1 },
            { name: 'scan_time', visible: true, sort_direction: 'desc', position: 2 }, // Sort desc
            { name: 'add_count', visible: true, sort_direction: 'none', position: 3 },
            { name: 'modify_count', visible: true, sort_direction: 'none', position: 4 },
            { name: 'delete_count', visible: true, sort_direction: 'none', position: 5 },
            { name: 'scan_state', visible: true, sort_direction: 'none', position: 6 },
          ],
          filters: [
            { column: 'scan_state', value: 'C,P,E' }, // Completed, Stopped, Error
          ],
          limit: 5, // Recent 5 scans
        })

        // Parse scan data
        const scanData: ScanRow[] = scansResponse.rows.map((row) => ({
          scan_id: parseInt(row[0]),
          root_id: parseInt(row[1]),
          scan_time: parseInt(row[2]),
          add_count: parseInt(row[3]),
          modify_count: parseInt(row[4]),
          delete_count: parseInt(row[5]),
          scan_state: row[6],
        }))

        setScans(scanData)
      } catch (err) {
        console.error('Error loading recent scans:', err)
        setError(err instanceof Error ? err.message : 'Failed to load recent scans')
      } finally {
        setLoading(false)
      }
    }

    loadData()
  }, [lastScanCompletedAt])

  const formatChanges = (add: number, modify: number, del: number): string => {
    const changes = []
    if (add > 0) changes.push(`${add} ${add === 1 ? 'add' : 'adds'}`)
    if (modify > 0) changes.push(`${modify} ${modify === 1 ? 'mod' : 'mods'}`)
    if (del > 0) changes.push(`${del} ${del === 1 ? 'del' : 'dels'}`)

    if (changes.length === 0) return 'No changes'
    return changes.join(', ')
  }

  const getStatusIcon = (state: string) => {
    switch (state) {
      case 'C':
        return <CheckCircle className="h-4 w-4 text-green-500" />
      case 'E':
        return <XCircle className="h-4 w-4 text-red-500" />
      case 'P':
        return <AlertTriangle className="h-4 w-4 text-orange-500" />
      default:
        return null
    }
  }

  const getStatusText = (state: string) => {
    switch (state) {
      case 'C':
        return 'Completed'
      case 'E':
        return 'Error'
      case 'P':
        return 'Stopped'
      default:
        return state
    }
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
          <CardTitle>Recent Scans</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-muted-foreground text-center py-4">
            Loading recent scans...
          </p>
        </CardContent>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Recent Scans</CardTitle>
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
          <CardTitle>Recent Scans</CardTitle>
        </CardHeader>
        <CardContent className="p-6">
          <div className="border border-border rounded-lg">
            <p className="text-sm text-muted-foreground text-center py-12">
              No completed scans yet
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
          <CardTitle>Recent Scans</CardTitle>
        </CardHeader>
        <CardContent className="p-6">
          <div className="border border-border rounded-lg overflow-hidden">
            <Table>
            <TableHeader className="bg-muted">
            <TableRow>
              <TableHead className="uppercase text-xs tracking-wide">Time</TableHead>
              <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
              <TableHead className="text-center uppercase text-xs tracking-wide">Changes</TableHead>
              <TableHead className="text-right uppercase text-xs tracking-wide">Status</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {scans.map((scan) => (
              <TableRow
                key={scan.scan_id}
                className="cursor-pointer hover:bg-muted/50"
              >
                <TableCell className="font-medium">
                  {formatTimeAgo(scan.scan_time)}
                </TableCell>
                <TableCell
                  className="max-w-[200px] truncate"
                  title={roots[scan.root_id] || `Root ${scan.root_id}`}
                >
                  <button
                    onClick={(e) => {
                      e.stopPropagation()
                      setSelectedRoot({ id: scan.root_id, path: roots[scan.root_id] || `Root ${scan.root_id}` })
                      setRootSheetOpen(true)
                    }}
                    className="text-left hover:underline hover:text-primary cursor-pointer"
                  >
                    {shortenPath(roots[scan.root_id] || `Root ${scan.root_id}`)}
                  </button>
                </TableCell>
                <TableCell className="text-center">
                  {formatChanges(scan.add_count, scan.modify_count, scan.delete_count)}
                </TableCell>
                <TableCell className="text-right">
                  <div className="flex items-center justify-end gap-2">
                    {getStatusIcon(scan.scan_state)}
                    <span className="text-sm">{getStatusText(scan.scan_state)}</span>
                  </div>
                </TableCell>
              </TableRow>
            ))}
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
