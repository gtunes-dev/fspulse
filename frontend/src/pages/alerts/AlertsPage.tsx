import { useState, useEffect, useCallback, useRef, useMemo } from 'react'
import { Info, X } from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { RootCard } from '@/components/shared/RootCard'
import { ItemDetailSheet } from '@/components/shared/ItemDetailSheet'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
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
import {
  updateAlertStatus,
  bulkUpdateAlertStatus,
  bulkUpdateAlertStatusByFilter,
  fetchMetadata,
  fetchQuery,
} from '@/lib/api'
import { formatTimeAgo } from '@/lib/dateUtils'
import type { AlertStatusValue, AlertTypeValue, ColumnState, ColumnSpec, FilterSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
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

const STATUS_LABELS: Record<string, string> = {
  'all': 'All Status',
  'O': 'Open',
  'F': 'Flagged',
  'D': 'Dismissed',
}

const TYPE_LABELS: Record<string, string> = {
  'all': 'All Types',
  'H': 'Suspicious Hash',
  'I': 'Invalid Item',
  'A': 'Access Denied',
}

const ACTION_LABELS: Record<AlertStatusValue, string> = {
  'D': 'Dismiss',
  'F': 'Flag',
  'O': 'Open',
}

export function AlertsPage() {
  const [selectedRootId, setSelectedRootId] = useState<string>('all')
  const [roots, setRoots] = useState<Root[]>([])
  const [loading, setLoading] = useState(true)
  const [columns, setColumns] = useState<ColumnState[]>([])
  const [statusFilter, setStatusFilter] = useState<string>('O') // Default to "Open"
  const [typeFilter, setTypeFilter] = useState<string>('all')
  const [pathSearch, setPathSearch] = useState('')
  const [debouncedPathSearch, setDebouncedPathSearch] = useState('')
  const [currentPage, setCurrentPage] = useState(1)
  const [error, setError] = useState<string | null>(null)
  const [alerts, setAlerts] = useState<AlertRow[]>([])
  const [totalCount, setTotalCount] = useState(0)
  const searchDebounceRef = useRef<number | null>(null)
  const [updatingAlertId, setUpdatingAlertId] = useState<number | null>(null)
  const [selectedItem, setSelectedItem] = useState<{ itemId: number; itemPath: string; rootId: number; scanId: number } | null>(null)
  const [sheetOpen, setSheetOpen] = useState(false)

  // Selection state — just a set of checked alert IDs
  const [selectedIds, setSelectedIds] = useState<Set<number>>(new Set())
  const [bulkUpdating, setBulkUpdating] = useState(false)

  // Confirmation dialog state (for "All matching" bulk actions)
  const [confirmDialog, setConfirmDialog] = useState<{
    open: boolean
    status: AlertStatusValue
  }>({ open: false, status: 'D' })

  // Track last filter key to avoid redundant count queries
  const lastFilterKeyRef = useRef<string>('')

  // Build the current filters array
  const buildFilters = useCallback((): FilterSpec[] => {
    const filters: FilterSpec[] = []
    if (selectedRootId !== 'all') {
      filters.push({ column: 'root_id', value: selectedRootId })
    }
    if (statusFilter && statusFilter !== 'all') {
      filters.push({ column: 'alert_status', value: statusFilter })
    }
    if (typeFilter && typeFilter !== 'all') {
      filters.push({ column: 'alert_type', value: typeFilter })
    }
    if (debouncedPathSearch.trim()) {
      filters.push({ column: 'item_path', value: `'${debouncedPathSearch.trim()}'` })
    }
    return filters
  }, [selectedRootId, statusFilter, typeFilter, debouncedPathSearch])

  // Clear selection when filters change
  useEffect(() => {
    setSelectedIds(new Set())
  }, [statusFilter, typeFilter, debouncedPathSearch, selectedRootId])

  // Compute header checkbox state
  const pageAlertIds = useMemo(() => alerts.map(a => a.alert_id), [alerts])

  const headerCheckboxState = useMemo((): boolean | 'indeterminate' => {
    if (alerts.length === 0) return false
    const selectedOnPage = pageAlertIds.filter(id => selectedIds.has(id))
    if (selectedOnPage.length === 0) return false
    if (selectedOnPage.length === pageAlertIds.length) return true
    return 'indeterminate'
  }, [alerts, pageAlertIds, selectedIds])

  const allOnPageSelected = useMemo(() => {
    return alerts.length > 0 && pageAlertIds.every(id => selectedIds.has(id))
  }, [alerts, pageAlertIds, selectedIds])

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
        setLoading(true)
        const columns: ColumnSpec[] = [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'asc', position: 1 },
        ]

        const response = await fetchQuery('roots', {
          columns,
          filters: [],
          limit: 1000,
          offset: 0,
        })

        const rootsData: Root[] = response.rows.map((row) => ({
          root_id: parseInt(row[0]),
          root_path: row[1],
        }))

        setRoots(rootsData)
      } catch (err) {
        console.error('Error loading roots:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRoots()
  }, [])

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

      const filters = buildFilters()

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
      const sortedCols = columns.filter(c => c.visible).sort((a, b) => a.position - b.position)
      const colIndexMap: Record<string, number> = {}
      sortedCols.forEach((col, idx) => {
        colIndexMap[col.name] = idx
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
  }, [columns, buildFilters, currentPage])

  // Load alerts when filters or page changes
  useEffect(() => {
    loadAlerts()
  }, [loadAlerts])

  // Reset to page 1 when filters change
  useEffect(() => {
    setCurrentPage(1)
  }, [statusFilter, typeFilter, debouncedPathSearch, selectedRootId])

  const handlePathSearchChange = (value: string) => {
    setPathSearch(value)
    if (searchDebounceRef.current) {
      clearTimeout(searchDebounceRef.current)
    }
    searchDebounceRef.current = window.setTimeout(() => {
      setDebouncedPathSearch(value)
    }, 500)
  }

  const handleStatusUpdate = async (alertId: number, newStatus: AlertStatusValue) => {
    setUpdatingAlertId(alertId)
    try {
      await updateAlertStatus(alertId, { status: newStatus })
      // Remove from selection since the user handled this alert individually
      if (selectedIds.has(alertId)) {
        setSelectedIds(prev => {
          const next = new Set(prev)
          next.delete(alertId)
          return next
        })
      }
      // Re-query so the view stays consistent with active filters
      lastFilterKeyRef.current = '' // Force count refresh
      await loadAlerts()
    } catch (err) {
      console.error('Error updating alert status:', err)
      alert('Failed to update alert status. Please try again.')
      loadAlerts()
    } finally {
      setUpdatingAlertId(null)
    }
  }

  // Selection handlers
  const handleHeaderCheckboxChange = () => {
    if (allOnPageSelected) {
      // Deselect all on this page (keep cross-page selections)
      setSelectedIds(prev => {
        const next = new Set(prev)
        pageAlertIds.forEach(id => next.delete(id))
        return next
      })
    } else {
      // Select all on page
      setSelectedIds(prev => {
        const next = new Set(prev)
        pageAlertIds.forEach(id => next.add(id))
        return next
      })
    }
  }

  const handleRowCheckboxChange = (alertId: number) => {
    setSelectedIds(prev => {
      const next = new Set(prev)
      if (next.has(alertId)) {
        next.delete(alertId)
      } else {
        next.add(alertId)
      }
      return next
    })
  }

  const clearSelection = () => {
    setSelectedIds(new Set())
  }

  // Bulk action on checked IDs
  const handleBulkAction = async (newStatus: AlertStatusValue) => {
    setBulkUpdating(true)
    try {
      await bulkUpdateAlertStatus({
        alert_ids: Array.from(selectedIds),
        status: newStatus,
      })
      clearSelection()
      lastFilterKeyRef.current = '' // Force count refresh
      await loadAlerts()
    } catch (err) {
      console.error('Error bulk updating alerts:', err)
      alert('Failed to update alerts. Please try again.')
    } finally {
      setBulkUpdating(false)
    }
  }

  // "All matching" action → confirmation dialog → filter-based action
  const handleAllMatchingAction = (newStatus: AlertStatusValue) => {
    setConfirmDialog({ open: true, status: newStatus })
  }

  const handleConfirmBulkAction = async () => {
    setBulkUpdating(true)
    try {
      await bulkUpdateAlertStatusByFilter({
        status: confirmDialog.status,
        status_filter: statusFilter !== 'all' ? statusFilter : undefined,
        type_filter: typeFilter !== 'all' ? typeFilter : undefined,
        root_id: selectedRootId !== 'all' ? Number(selectedRootId) : undefined,
        item_path: debouncedPathSearch.trim() || undefined,
      })
      setConfirmDialog({ open: false, status: 'D' })
      clearSelection()
      lastFilterKeyRef.current = '' // Force count refresh
      await loadAlerts()
    } catch (err) {
      console.error('Error bulk updating alerts by filter:', err)
      alert('Failed to update alerts. Please try again.')
    } finally {
      setBulkUpdating(false)
    }
  }

  const getAlertTypeBadge = (type: AlertTypeValue) => {
    switch (type) {
      case 'H':
        return <Badge variant="error">Suspicious Hash</Badge>
      case 'I':
        return <Badge variant="error">Invalid Item</Badge>
      case 'A':
        return <Badge variant="warning">Access Denied</Badge>
    }
  }

  const getAlertDetails = (alertRow: AlertRow) => {
    switch (alertRow.alert_type) {
      case 'H':
        return (
          <div className="text-xs space-y-1">
            <div>Hash changed</div>
            <div className="font-mono text-muted-foreground">Old: {alertRow.hash_old || 'N/A'}</div>
            <div className="font-mono text-muted-foreground">New: {alertRow.hash_new || 'N/A'}</div>
          </div>
        )
      case 'I':
        return <div className="text-xs">{alertRow.val_error || 'Validation error'}</div>
      case 'A':
        return <div className="text-xs">File could not be read</div>
    }
  }

  // Build filter summary for confirmation dialog
  const getFilterSummary = () => {
    const items: string[] = []
    items.push(`Status: ${STATUS_LABELS[statusFilter] || statusFilter}`)
    items.push(`Type: ${TYPE_LABELS[typeFilter] || typeFilter}`)
    if (selectedRootId !== 'all') {
      const root = roots.find(r => r.root_id === parseInt(selectedRootId))
      items.push(`Root: ${root?.root_path || selectedRootId}`)
    } else {
      items.push('Root: All Roots')
    }
    if (debouncedPathSearch.trim()) {
      items.push(`Path contains: "${debouncedPathSearch.trim()}"`)
    }
    return items
  }

  const start = (currentPage - 1) * ITEMS_PER_PAGE + 1
  const end = Math.min(start + ITEMS_PER_PAGE - 1, totalCount)
  const colSpan = 9 // checkbox + 8 data columns

  if (loading && roots.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-8">Alerts</h1>

      <div className="flex-1">
        <RootCard
          roots={roots}
          selectedRootId={selectedRootId}
          onRootChange={setSelectedRootId}
          allowAll={true}
          actionBar={
            <>
              <div className="flex items-center gap-3">
                <span className="text-sm font-medium text-muted-foreground">Status:</span>
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

              <div className="flex items-center gap-3">
                <span className="text-sm font-medium text-muted-foreground">Type:</span>
                <Select value={typeFilter} onValueChange={setTypeFilter}>
                  <SelectTrigger className="w-[180px]">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="all">All Types</SelectItem>
                    <SelectItem value="H">Suspicious Hash</SelectItem>
                    <SelectItem value="I">Invalid Item</SelectItem>
                    <SelectItem value="A">Access Denied</SelectItem>
                  </SelectContent>
                </Select>
              </div>

              <SearchFilter
                value={pathSearch}
                onChange={handlePathSearchChange}
              />
            </>
          }
        >
          {/* Status Action Bar — always visible, scope adapts to selection state */}
          <div className="bg-muted/50 border border-border rounded-lg px-4 py-3">
            <div className="flex items-center gap-4">
              <span className="text-sm font-medium">
                {selectedIds.size > 0
                  ? `${selectedIds.size.toLocaleString()} Selected`
                  : `${totalCount.toLocaleString()} Matching Alerts`
                }
              </span>
              <div className="h-5 w-px bg-border" />
              {selectedIds.size > 0 ? (
                <>
                  <div className="flex items-center gap-2">
                    <Button onClick={() => handleBulkAction('D')} disabled={bulkUpdating}>
                      Dismiss
                    </Button>
                    <Button onClick={() => handleBulkAction('F')} disabled={bulkUpdating}>
                      Flag
                    </Button>
                    <Button onClick={() => handleBulkAction('O')} disabled={bulkUpdating}>
                      Open
                    </Button>
                  </div>
                  <Button
                    variant="ghost"
                    onClick={clearSelection}
                    disabled={bulkUpdating}
                    className="text-muted-foreground"
                  >
                    <X className="h-4 w-4 mr-1" />
                    Clear
                  </Button>
                </>
              ) : (
                <div className="flex items-center gap-2">
                  <Button
                    onClick={() => handleAllMatchingAction('D')}
                    disabled={totalCount === 0 || bulkUpdating}
                  >
                    Dismiss All
                  </Button>
                  <Button
                    onClick={() => handleAllMatchingAction('F')}
                    disabled={totalCount === 0 || bulkUpdating}
                  >
                    Flag All
                  </Button>
                  <Button
                    onClick={() => handleAllMatchingAction('O')}
                    disabled={totalCount === 0 || bulkUpdating}
                  >
                    Open All
                  </Button>
                </div>
              )}
            </div>
          </div>

          {/* Bordered Table */}
          <div className="border border-border rounded-lg">
            <div className="p-0">
              <Table>
                <TableHeader>
                  <TableRow>
                    <TableHead className="bg-muted w-[40px] text-center">
                      <Checkbox
                        checked={headerCheckboxState}
                        onCheckedChange={handleHeaderCheckboxChange}
                        disabled={loading || alerts.length === 0}
                        aria-label="Select all alerts on this page"
                      />
                    </TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[120px]">Status</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[180px]">Alert Type</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[80px]">Root ID</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[80px]">Item ID</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[80px]">Scan ID</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted w-[250px]">File</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center">Details</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide bg-muted text-center w-[110px]">Created</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {loading ? (
                    <TableRow>
                      <TableCell colSpan={colSpan} className="text-center py-8 text-muted-foreground">
                        Loading...
                      </TableCell>
                    </TableRow>
                  ) : error ? (
                    <TableRow>
                      <TableCell colSpan={colSpan} className="text-center py-8 text-red-600">
                        {error}
                      </TableCell>
                    </TableRow>
                  ) : alerts.length === 0 ? (
                    <TableRow>
                      <TableCell colSpan={colSpan} className="text-center py-8 text-muted-foreground">
                        No alerts found.
                      </TableCell>
                    </TableRow>
                  ) : (
                    alerts.map((alertRow) => (
                      <TableRow key={alertRow.alert_id}>
                        <TableCell className="text-center">
                          <Checkbox
                            checked={selectedIds.has(alertRow.alert_id)}
                            onCheckedChange={() => handleRowCheckboxChange(alertRow.alert_id)}
                            aria-label={`Select alert ${alertRow.alert_id}`}
                          />
                        </TableCell>
                        <TableCell>
                          <Select
                            value={alertRow.alert_status}
                            onValueChange={(value) => handleStatusUpdate(alertRow.alert_id, value as AlertStatusValue)}
                            disabled={updatingAlertId === alertRow.alert_id}
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
                        <TableCell className="text-center">{getAlertTypeBadge(alertRow.alert_type)}</TableCell>
                        <TableCell className="text-center text-muted-foreground">{alertRow.root_id}</TableCell>
                        <TableCell className="text-center text-muted-foreground">{alertRow.item_id}</TableCell>
                        <TableCell className="text-center text-muted-foreground">{alertRow.scan_id}</TableCell>
                        <TableCell>
                          <div
                            className="group flex items-center gap-2 cursor-pointer hover:bg-accent/50 -mx-2 px-2 py-1 rounded transition-colors"
                            onClick={() => {
                              setSelectedItem({ itemId: alertRow.item_id, itemPath: alertRow.item_path, rootId: alertRow.root_id, scanId: alertRow.scan_id })
                              setSheetOpen(true)
                            }}
                            title={alertRow.item_path}
                          >
                            <Info className="h-5 w-5 flex-shrink-0 text-muted-foreground group-hover:text-primary transition-colors translate-y-[0.5px]" />
                            <span className="font-mono text-sm group-hover:text-foreground group-hover:underline transition-colors truncate">
                              {alertRow.item_name}
                            </span>
                          </div>
                        </TableCell>
                        <TableCell>{getAlertDetails(alertRow)}</TableCell>
                        <TableCell className="text-center text-sm text-muted-foreground">
                          {formatTimeAgo(alertRow.created_at)}
                        </TableCell>
                      </TableRow>
                    ))
                  )}
                </TableBody>
              </Table>
            </div>
          </div>

          {/* Pagination */}
          <div className="flex items-center justify-between">
            <div className="text-sm text-muted-foreground whitespace-nowrap">
              Showing {(totalCount > 0 ? start : 0).toLocaleString()} - {end.toLocaleString()} of {totalCount.toLocaleString()} alerts
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
        </RootCard>

        {/* Item Detail Sheet */}
        {selectedItem && (
          <ItemDetailSheet
            itemId={selectedItem.itemId}
            itemPath={selectedItem.itemPath}
            itemType="F"
            isTombstone={false}
            rootId={selectedItem.rootId}
            scanId={selectedItem.scanId}
            open={sheetOpen}
            onOpenChange={setSheetOpen}
          />
        )}

        {/* Confirmation Dialog for Actions dropdown (filter-based bulk actions) */}
        <Dialog
          open={confirmDialog.open}
          onOpenChange={(open) => setConfirmDialog(prev => ({ ...prev, open }))}
        >
          <DialogContent>
            <DialogHeader>
              <DialogTitle>
                {ACTION_LABELS[confirmDialog.status]} All Matching Alerts?
              </DialogTitle>
              <DialogDescription>
                This will {ACTION_LABELS[confirmDialog.status].toLowerCase()} {totalCount.toLocaleString()} alerts
                matching your current filters:
              </DialogDescription>
            </DialogHeader>
            <ul className="list-disc list-inside text-sm text-muted-foreground space-y-1 py-2">
              {getFilterSummary().map((item, i) => (
                <li key={i}>{item}</li>
              ))}
            </ul>
            <DialogFooter>
              <Button
                variant="outline"
                onClick={() => setConfirmDialog(prev => ({ ...prev, open: false }))}
                disabled={bulkUpdating}
              >
                Cancel
              </Button>
              <Button onClick={handleConfirmBulkAction} disabled={bulkUpdating}>
                {bulkUpdating
                  ? 'Updating...'
                  : `${ACTION_LABELS[confirmDialog.status]} ${totalCount.toLocaleString()}`
                }
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
    </div>
  )
}
