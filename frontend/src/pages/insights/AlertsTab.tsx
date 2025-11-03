import { useState, useEffect, useCallback, useRef } from 'react'
import { RefreshCw, Info } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { SearchFilter } from '@/components/ui/SearchFilter'
import { RootPicker } from '@/components/ui/RootPicker'
import { ItemDetailSheet } from '@/components/browse/ItemDetailSheet'
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

interface Root {
  root_id: number
  root_path: string
}

interface AlertsTabProps {
  contextFilter: ContextFilterType
  contextValue: string
  roots: Root[]
  onContextFilterChange: (value: ContextFilterType) => void
  onContextValueChange: (value: string) => void
}

interface AlertRow {
  alert_id: number
  alert_type: AlertTypeValue
  alert_status: AlertStatusValue
  root_id: number
  item_id: number
  scan_id: number
  item_path: string
  item_name: string
  created_at: number
  hash_old: string | null
  hash_new: string | null
  val_error: string | null
}

const ITEMS_PER_PAGE = 25

export function AlertsTab({ contextFilter, contextValue, roots, onContextFilterChange, onContextValueChange }: AlertsTabProps) {
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
  const [selectedItem, setSelectedItem] = useState<{ itemId: number; itemPath: string; rootId: number } | null>(null)
  const [sheetOpen, setSheetOpen] = useState(false)

  // Track last filter key to avoid redundant count queries
  const lastFilterKeyRef = useRef<string>('')

  // Load metadata on mount
  useEffect(() => {
    async function loadMetadata() {
      try {
        const metadata = await fetchMetadata('alerts')
        const columnState: ColumnState[] = []

        metadata.columns.forEach((col) => {
          columnState.push({
            ...col,
            visible: true,
            sort_direction: col.name === 'created_at' ? 'desc' : 'none',
            position: columnState.length,
          })

          // Add item_path@name column right after item_path for display
          if (col.name === 'item_path') {
            columnState.push({
              ...col,
              name: 'item_path@name',
              display_name: 'File Name',
              visible: true,
              sort_direction: 'none',
              position: columnState.length,
            })
          }
        })

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
      const needsCount = filterKey !== lastFilterKeyRef.current

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
        lastFilterKeyRef.current = filterKey
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

      // Build index map from the columns WE sent (which include format specifiers like @name)
      // This way we can distinguish between item_path and item_path@name
      const sortedCols = columns.filter(c => c.visible).sort((a, b) => a.position - b.position)
      const colIndexMap: Record<string, number> = {}
      sortedCols.forEach((col, idx) => {
        colIndexMap[col.name] = idx  // Uses full name like "item_path@name"
      })

      // Map response to AlertRow format
      const rows: AlertRow[] = (fetchData.rows || []).map((row: string[]) => {
        return {
          alert_id: parseInt(row[colIndexMap['alert_id']]),
          alert_type: row[colIndexMap['alert_type']] as AlertTypeValue,
          alert_status: row[colIndexMap['alert_status']] as AlertStatusValue,
          root_id: parseInt(row[colIndexMap['root_id']]),
          item_id: parseInt(row[colIndexMap['item_id']]),
          scan_id: parseInt(row[colIndexMap['scan_id']]),
          item_path: row[colIndexMap['item_path']],
          item_name: row[colIndexMap['item_path@name']],
          created_at: parseInt(row[colIndexMap['created_at']]),
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
      {/* Combined Filter Toolbar - Two Rows */}
      <div className="mb-4">
        <div className="flex flex-col gap-4 px-6 py-4 bg-background rounded-xl border-2 border-border/60 shadow-lg shadow-black/5 dark:shadow-black/20">
          {/* First Row - Context Filter */}
          <div className="flex items-center gap-5">
            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-muted-foreground/80">Context:</span>
              <Select value={contextFilter} onValueChange={onContextFilterChange}>
                <SelectTrigger className="w-[180px] shadow-sm ring-1 ring-border/50 hover:ring-border transition-all">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Data</SelectItem>
                  <SelectItem value="root">By Root</SelectItem>
                  <SelectItem value="scan">By Scan ID</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {contextFilter === 'root' && (
              <RootPicker
                roots={roots}
                value={contextValue}
                onChange={onContextValueChange}
                placeholder="Select a root"
              />
            )}

            {contextFilter === 'scan' && (
              <Input
                type="text"
                value={contextValue}
                onChange={(e) => onContextValueChange(e.target.value)}
                placeholder="Enter scan ID..."
                className="flex-1 max-w-md shadow-sm ring-1 ring-border/50 hover:ring-border transition-all"
              />
            )}
          </div>

          {/* Second Row - Alert Filters */}
          <div className="flex items-center gap-5">
            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-muted-foreground/80">Status:</span>
              <Select value={statusFilter} onValueChange={setStatusFilter}>
                <SelectTrigger className="w-[150px] shadow-sm ring-1 ring-border/50 hover:ring-border transition-all">
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

            <div className="flex items-center gap-3">
              <span className="text-sm font-semibold text-muted-foreground/80">Type:</span>
              <Select value={typeFilter} onValueChange={setTypeFilter}>
                <SelectTrigger className="w-[180px] shadow-sm ring-1 ring-border/50 hover:ring-border transition-all">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Types</SelectItem>
                  <SelectItem value="H">Suspicious Hash</SelectItem>
                  <SelectItem value="I">Invalid Item</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <SearchFilter
              value={pathSearch}
              onChange={handlePathSearchChange}
            />

            <Button
              variant="outline"
              size="icon"
              onClick={() => loadAlerts()}
              title="Refresh"
              className="shadow-sm ring-1 ring-border/50 hover:ring-border transition-all"
            >
              <RefreshCw className="h-4 w-4" />
            </Button>
          </div>
        </div>
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
                <TableHead className="text-center w-[80px]">ITEM ID</TableHead>
                <TableHead className="text-center w-[80px]">SCAN ID</TableHead>
                <TableHead className="w-[250px]">FILE</TableHead>
                <TableHead className="text-center">DETAILS</TableHead>
                <TableHead className="text-center w-[110px]">CREATED</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {loading ? (
                <TableRow>
                  <TableCell colSpan={8} className="text-center py-8 text-muted-foreground">
                    Loading...
                  </TableCell>
                </TableRow>
              ) : error ? (
                <TableRow>
                  <TableCell colSpan={8} className="text-center py-8 text-red-600">
                    {error}
                  </TableCell>
                </TableRow>
              ) : alerts.length === 0 ? (
                <TableRow>
                  <TableCell colSpan={8} className="text-center py-8 text-muted-foreground">
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
                    <TableCell className="text-center text-muted-foreground">{alert.item_id}</TableCell>
                    <TableCell className="text-center text-muted-foreground">{alert.scan_id}</TableCell>
                    <TableCell>
                      <div
                        className="group flex items-center gap-2 cursor-pointer hover:bg-accent/50 -mx-2 px-2 py-1 rounded transition-colors"
                        onClick={() => {
                          setSelectedItem({ itemId: alert.item_id, itemPath: alert.item_path, rootId: alert.root_id })
                          setSheetOpen(true)
                        }}
                        title={alert.item_path}
                      >
                        <Info className="h-5 w-5 flex-shrink-0 text-muted-foreground group-hover:text-primary transition-colors translate-y-[0.5px]" />
                        <span className="font-mono text-sm group-hover:text-foreground group-hover:underline transition-colors truncate">
                          {alert.item_name}
                        </span>
                      </div>
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

      {/* Item Detail Sheet */}
      {selectedItem && (
        <ItemDetailSheet
          itemId={selectedItem.itemId}
          itemPath={selectedItem.itemPath}
          itemType="F"
          isTombstone={false}
          rootId={selectedItem.rootId}
          open={sheetOpen}
          onOpenChange={setSheetOpen}
        />
      )}
    </div>
  )
}
