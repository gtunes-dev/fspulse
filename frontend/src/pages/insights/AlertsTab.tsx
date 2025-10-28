import { useState, useEffect, useCallback } from 'react'
import { RefreshCw } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { Card, CardContent } from '@/components/ui/card'
import { updateAlertStatus, fetchMetadata } from '@/lib/api'
import { formatTimeAgo } from '@/lib/dateUtils'
import type { AlertStatusValue, AlertTypeValue, ContextFilterType, ColumnState } from '@/lib/types'

interface AlertsTabProps {
  contextFilter: ContextFilterType
  contextValue: string
}

interface AlertRow {
  alert_id: number
  alert_type: AlertTypeValue
  alert_status: AlertStatusValue
  root_id: number
  scan_id: number
  item_path: string
  created_at: number
  hash_old: string | null
  hash_new: string | null
  val_error: string | null
}

const ITEMS_PER_PAGE = 25

export function AlertsTab({ contextFilter, contextValue }: AlertsTabProps) {
  const [columns, setColumns] = useState<ColumnState[]>([])
  const [statusFilter, setStatusFilter] = useState<string>('O') // Default to "Open"
  const [typeFilter, setTypeFilter] = useState<string>('all')
  const [pathSearch, setPathSearch] = useState('')
  const [currentPage, setCurrentPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [alerts, setAlerts] = useState<AlertRow[]>([])
  const [totalCount, setTotalCount] = useState(0)
  const [searchDebounce, setSearchDebounce] = useState<number | null>(null)
  const [updatingAlertId, setUpdatingAlertId] = useState<number | null>(null)

  // Load metadata on mount
  useEffect(() => {
    async function loadMetadata() {
      try {
        const metadata = await fetchMetadata('alerts')
        const columnState: ColumnState[] = metadata.columns.map((col, index) => ({
          ...col,
          visible: true,
          sort_direction: col.name === 'created_at' ? 'desc' : 'none',
          position: index,
        }))
        setColumns(columnState)
      } catch (err) {
        console.error('Error loading metadata:', err)
      }
    }
    loadMetadata()
  }, [])

  const loadAlerts = useCallback(async () => {
    if (columns.length === 0) return

    try {
      setLoading(true)
      setError(null)

      // Build filters array
      const filters: Array<{ column: string; value: string }> = []

      // Add context filter if applicable
      if (contextFilter !== 'all' && contextValue.trim()) {
        if (contextFilter === 'root') {
          filters.push({ column: 'root_id', value: contextValue.trim() })
        } else if (contextFilter === 'scan') {
          filters.push({ column: 'scan_id', value: contextValue.trim() })
        }
      }

      // Add status filter
      if (statusFilter && statusFilter !== 'all') {
        filters.push({ column: 'alert_status', value: statusFilter })
      }

      // Add type filter
      if (typeFilter && typeFilter !== 'all') {
        filters.push({ column: 'alert_type', value: typeFilter })
      }

      // Add path search
      if (pathSearch.trim()) {
        filters.push({ column: 'item_path', value: `'${pathSearch.trim()}'` })
      }

      // Build filter key to detect when filters change
      const filterKey = JSON.stringify(filters)
      const needsCount = filterKey !== (loadAlerts as any).lastFilterKey

      // Get count only when filters change
      if (needsCount) {
        const countResponse = await fetch('/api/query/alerts/count', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({
            columns: [],  // Count doesn't need column specs
            filters,
            limit: 0,
            offset: 0,
          }),
        })

        if (!countResponse.ok) {
          throw new Error(`Count query failed: ${countResponse.statusText}`)
        }

        const countData = await countResponse.json()
        setTotalCount(countData.count)
        ;(loadAlerts as any).lastFilterKey = filterKey
      }

      // Always fetch current page
      const fetchResponse = await fetch('/api/query/alerts/fetch', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          columns: columns.map((col) => ({
            name: col.name,
            visible: col.visible,
            sort_direction: col.sort_direction,
            position: col.position,
          })),
          filters,
          limit: ITEMS_PER_PAGE,
          offset: (currentPage - 1) * ITEMS_PER_PAGE,
        }),
      })

      if (!fetchResponse.ok) {
        throw new Error(`Fetch query failed: ${fetchResponse.statusText}`)
      }

      const fetchData = await fetchResponse.json()

      // Map response to AlertRow format
      const rows: AlertRow[] = (fetchData.rows || []).map((row: string[]) => {
        const colIndexMap: Record<string, number> = {}
        fetchData.columns.forEach((colName: string, idx: number) => {
          colIndexMap[colName] = idx
        })

        // Parse created_at as Unix timestamp in seconds (backend returns raw timestamp with @timestamp format modifier)
        const createdAtTimestamp = parseInt(row[colIndexMap['created_at']])

        return {
          alert_id: parseInt(row[colIndexMap['alert_id']]),
          alert_type: row[colIndexMap['alert_type']] as AlertTypeValue,
          alert_status: row[colIndexMap['alert_status']] as AlertStatusValue,
          root_id: parseInt(row[colIndexMap['root_id']]),
          scan_id: parseInt(row[colIndexMap['scan_id']]),
          item_path: row[colIndexMap['item_path']],
          created_at: createdAtTimestamp,
          hash_old: row[colIndexMap['hash_old']] || null,
          hash_new: row[colIndexMap['hash_new']] || null,
          val_error: row[colIndexMap['val_error']] || null,
        }
      })

      setAlerts(rows)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load alerts')
      console.error('Error loading alerts:', err)
    } finally {
      setLoading(false)
    }
  }, [columns, contextFilter, contextValue, statusFilter, typeFilter, pathSearch, currentPage])

  // Load alerts when filters or page changes
  useEffect(() => {
    loadAlerts()
  }, [loadAlerts])

  // Reset to page 1 when filters change
  useEffect(() => {
    setCurrentPage(1)
  }, [statusFilter, typeFilter, pathSearch, contextFilter, contextValue])

  // Debounce path search
  const handlePathSearchChange = (value: string) => {
    setPathSearch(value)
    if (searchDebounce) {
      clearTimeout(searchDebounce)
    }
    const timeout = setTimeout(() => {
      // Trigger reload via useEffect
    }, 500)
    setSearchDebounce(timeout)
  }

  const handleStatusUpdate = async (alertId: number, newStatus: AlertStatusValue) => {
    setUpdatingAlertId(alertId)
    try {
      await updateAlertStatus(alertId, { status: newStatus })
      // Update local state
      setAlerts((prev) =>
        prev.map((alert) =>
          alert.alert_id === alertId ? { ...alert, alert_status: newStatus } : alert
        )
      )
    } catch (err) {
      console.error('Error updating alert status:', err)
      alert('Failed to update alert status. Please try again.')
      // Reload to reset
      loadAlerts()
    } finally {
      setUpdatingAlertId(null)
    }
  }

  const truncatePath = (path: string, maxLength: number): string => {
    if (path.length <= maxLength) return path
    const half = Math.floor(maxLength / 2)
    return `${path.slice(0, half)}...${path.slice(-half)}`
  }

  const getAlertTypeBadge = (type: AlertTypeValue) => {
    if (type === 'H') {
      return <Badge variant="error">Suspicious Hash</Badge>
    } else {
      return <Badge variant="error">Invalid Item</Badge>
    }
  }

  const getAlertDetails = (alert: AlertRow) => {
    if (alert.alert_type === 'H') {
      return (
        <div className="text-xs space-y-1">
          <div>Hash changed</div>
          <div className="font-mono text-muted-foreground">Old: {alert.hash_old || 'N/A'}</div>
          <div className="font-mono text-muted-foreground">New: {alert.hash_new || 'N/A'}</div>
        </div>
      )
    } else {
      return <div className="text-xs">{alert.val_error || 'Validation error'}</div>
    }
  }

  const start = (currentPage - 1) * ITEMS_PER_PAGE + 1
  const end = Math.min(start + ITEMS_PER_PAGE - 1, totalCount)

  return (
    <div className="flex flex-col gap-4">
      {/* Filter Toolbar */}
      <div className="flex items-center gap-4 p-4 bg-muted/30 rounded-lg">
        <div className="flex items-center gap-2">
          <label className="text-sm font-medium whitespace-nowrap">Status:</label>
          <Select value={statusFilter} onValueChange={setStatusFilter}>
            <SelectTrigger className="w-[150px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Status</SelectItem>
              <SelectItem value="O">Open</SelectItem>
              <SelectItem value="F">Flagged</SelectItem>
              <SelectItem value="D">Dismissed</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-2">
          <label className="text-sm font-medium whitespace-nowrap">Type:</label>
          <Select value={typeFilter} onValueChange={setTypeFilter}>
            <SelectTrigger className="w-[180px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Types</SelectItem>
              <SelectItem value="H">Suspicious Hash</SelectItem>
              <SelectItem value="I">Invalid Item</SelectItem>
            </SelectContent>
          </Select>
        </div>

        <div className="flex items-center gap-2 flex-1">
          <label className="text-sm font-medium whitespace-nowrap">Path:</label>
          <Input
            type="text"
            value={pathSearch}
            onChange={(e) => handlePathSearchChange(e.target.value)}
            placeholder="Search file paths..."
            className="flex-1"
          />
        </div>

        <Button
          variant="outline"
          size="icon"
          onClick={() => loadAlerts()}
          title="Refresh"
        >
          <RefreshCw className="h-4 w-4" />
        </Button>
      </div>

      {/* Alerts Table */}
      <Card>
        <CardContent className="p-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="text-center w-[120px]">STATUS</TableHead>
                <TableHead className="text-center w-[180px]">ALERT TYPE</TableHead>
                <TableHead className="text-center w-[80px]">ROOT ID</TableHead>
                <TableHead className="text-center w-[80px]">SCAN ID</TableHead>
                <TableHead className="text-center">FILE PATH</TableHead>
                <TableHead className="text-center w-[250px]">DETAILS</TableHead>
                <TableHead className="text-center w-[110px]">CREATED</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {loading ? (
                <TableRow>
                  <TableCell colSpan={7} className="text-center py-8 text-muted-foreground">
                    Loading...
                  </TableCell>
                </TableRow>
              ) : error ? (
                <TableRow>
                  <TableCell colSpan={7} className="text-center py-8 text-red-600">
                    {error}
                  </TableCell>
                </TableRow>
              ) : alerts.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={7} className="text-center py-8 text-muted-foreground">
                    No alerts found.
                  </TableCell>
                </TableRow>
              ) : (
                alerts.map((alert) => (
                  <TableRow key={alert.alert_id}>
                    <TableCell>
                      <Select
                        value={alert.alert_status}
                        onValueChange={(value) => handleStatusUpdate(alert.alert_id, value as AlertStatusValue)}
                        disabled={updatingAlertId === alert.alert_id}
                      >
                        <SelectTrigger className="w-[110px]">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="O">Open</SelectItem>
                          <SelectItem value="F">Flagged</SelectItem>
                          <SelectItem value="D">Dismissed</SelectItem>
                        </SelectContent>
                      </Select>
                    </TableCell>
                    <TableCell className="text-center">{getAlertTypeBadge(alert.alert_type)}</TableCell>
                    <TableCell className="text-center text-muted-foreground">{alert.root_id}</TableCell>
                    <TableCell className="text-center text-muted-foreground">{alert.scan_id}</TableCell>
                    <TableCell>
                      <span className="font-mono text-sm" title={alert.item_path}>
                        {truncatePath(alert.item_path, 60)}
                      </span>
                    </TableCell>
                    <TableCell>{getAlertDetails(alert)}</TableCell>
                    <TableCell className="text-center text-sm text-muted-foreground">
                      {formatTimeAgo(alert.created_at)}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>

          {/* Pagination */}
          <div className="flex items-center justify-between px-4 py-3 border-t">
            <div className="text-sm text-muted-foreground">
              Showing {totalCount > 0 ? start : 0} - {end} of {totalCount} alerts
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
        </CardContent>
      </Card>
    </div>
  )
}
