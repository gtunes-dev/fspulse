import { useState, useEffect, useCallback, useRef } from 'react'
import { GripVertical, Plus, X } from 'lucide-react'
import { fetchMetadata, countQuery, fetchQuery } from '@/lib/api'
import { FilterModal } from './FilterModal'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { formatDateFull } from '@/lib/dateUtils'
import type {
  ColumnState,
  ActiveFilter,
} from '@/lib/types'

const ITEMS_PER_PAGE = 25

interface DataExplorerViewProps {
  domain: string
}

export function DataExplorerView({ domain }: DataExplorerViewProps) {
  const [columns, setColumns] = useState<ColumnState[]>([])
  const [filters, setFilters] = useState<ActiveFilter[]>([])
  const [dataColumns, setDataColumns] = useState<string[]>([])
  const [dataRows, setDataRows] = useState<string[][]>([])
  const [totalCount, setTotalCount] = useState(0)
  const [currentPage, setCurrentPage] = useState(1)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [draggedColumn, setDraggedColumn] = useState<string | null>(null)
  const [dragOverColumn, setDragOverColumn] = useState<string | null>(null)
  const [isDraggingDown, setIsDraggingDown] = useState(false)
  const [filterModalColumn, setFilterModalColumn] = useState<ColumnState | null>(null)
  const lastFilterKeyRef = useRef<string>('')

  // Load column metadata on mount
  useEffect(() => {
    async function loadMetadata() {
      try {
        setLoading(true)
        const metadata = await fetchMetadata(domain)

        // Initialize column state from metadata
        const columnState: ColumnState[] = metadata.columns.map((col, index) => ({
          ...col,
          visible: col.is_default,
          sort_direction: 'none',
          position: index,
        }))

        setColumns(columnState)
        setError(null)
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load metadata')
      } finally {
        setLoading(false)
      }
    }

    loadMetadata()
  }, [domain])

  // Execute query when columns, filters, or page changes
  const loadData = useCallback(async () => {
    if (columns.length === 0) return

    try {
      setLoading(true)

      const columnSpecs = columns
        .filter((col) => col.visible)
        .sort((a, b) => a.position - b.position)
        .map((col) => ({
          name: col.name,
          visible: col.visible,
          sort_direction: col.sort_direction,
          position: col.position,
        }))

      const filterSpecs = filters.map((f) => ({
        column: f.column_name,
        value: f.filter_value,
      }))

      // Build filter key to detect when filters/columns change
      const filterKey = JSON.stringify({ columnSpecs, filterSpecs })
      const needsCount = filterKey !== lastFilterKeyRef.current

      // Get count only when filters or visible columns change
      if (needsCount) {
        const countData = await countQuery(domain, {
          columns: columnSpecs,
          filters: filterSpecs,
        })
        setTotalCount(countData.count)
        lastFilterKeyRef.current = filterKey
      }

      // Always fetch current page
      const fetchData = await fetchQuery(domain, {
        columns: columnSpecs,
        filters: filterSpecs,
        limit: ITEMS_PER_PAGE,
        offset: (currentPage - 1) * ITEMS_PER_PAGE,
      })

      setDataColumns(fetchData.columns)
      setDataRows(fetchData.rows)
      setError(null)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load data')
    } finally {
      setLoading(false)
    }
  }, [domain, columns, filters, currentPage])

  useEffect(() => {
    loadData()
  }, [loadData])

  // Drag-and-drop handlers
  const handleDragStart = (columnName: string) => {
    setDraggedColumn(columnName)
  }

  const handleDragOver = (e: React.DragEvent, targetColumnName: string) => {
    e.preventDefault() // Allow drop
    setDragOverColumn(targetColumnName)

    // Determine if we're dragging down or up
    if (draggedColumn) {
      const draggedIndex = columns.findIndex((c) => c.name === draggedColumn)
      const targetIndex = columns.findIndex((c) => c.name === targetColumnName)
      setIsDraggingDown(draggedIndex < targetIndex)
    }
  }

  const handleDragEnd = () => {
    setDraggedColumn(null)
    setDragOverColumn(null)
  }

  const handleDragLeave = () => {
    setDragOverColumn(null)
  }

  const handleDrop = (targetColumnName: string) => {
    if (!draggedColumn || draggedColumn === targetColumnName) {
      return
    }

    setColumns((prev) => {
      const newColumns = [...prev]
      const draggedIndex = newColumns.findIndex((c) => c.name === draggedColumn)
      const targetIndex = newColumns.findIndex((c) => c.name === targetColumnName)

      // Remove dragged column and insert at target position
      const [draggedCol] = newColumns.splice(draggedIndex, 1)
      newColumns.splice(targetIndex, 0, draggedCol)

      // Update position values
      return newColumns.map((col, index) => ({
        ...col,
        position: index,
      }))
    })

    setCurrentPage(1) // Reset to first page when order changes
    setDragOverColumn(null)
  }

  // Filter handlers
  const handleApplyFilter = (columnName: string, filterValue: string) => {
    const column = columns.find((c) => c.name === columnName)
    if (!column) return

    setFilters((prev) => {
      // Remove existing filter for this column if any
      const filtered = prev.filter((f) => f.column_name !== columnName)
      // Add new filter
      return [
        ...filtered,
        {
          column_name: columnName,
          display_name: column.display_name,
          filter_value: filterValue,
        },
      ]
    })
    setCurrentPage(1) // Reset to first page when filters change
  }

  const handleRemoveFilter = (columnName: string) => {
    setFilters((prev) => prev.filter((f) => f.column_name !== columnName))
    setCurrentPage(1)
  }

  if (loading && columns.length === 0) {
    return <div className="p-4">Loading metadata...</div>
  }

  if (error) {
    return <div className="p-4 text-destructive">Error: {error}</div>
  }

  return (
    <div className="flex h-full gap-4">
      {/* Column Selector Card - Left Panel */}
      <Card className="w-96 flex flex-col">
        <CardContent className="flex-1 overflow-y-auto p-0">
          <table className="w-full text-sm">
            <thead className="sticky top-0 bg-muted">
              <tr className="border-b border-border">
                <th className="text-center font-medium py-2 px-2 w-8"></th>
                <th className="text-center font-medium py-2 px-2 w-8"></th>
                <th className="text-center font-medium py-2 uppercase text-xs tracking-wide">COLUMN</th>
                <th className="text-center font-medium py-2 w-16 uppercase text-xs tracking-wide">SORT</th>
                <th className="text-center font-medium py-2 w-16 uppercase text-xs tracking-wide">FILTER</th>
              </tr>
            </thead>
            <tbody>
              {columns
                .slice()
                .sort((a, b) => a.position - b.position)
                .map((col) => (
                  <tr
                    key={col.name}
                    draggable
                    onDragStart={() => handleDragStart(col.name)}
                    onDragOver={(e) => handleDragOver(e, col.name)}
                    onDragLeave={handleDragLeave}
                    onDragEnd={handleDragEnd}
                    onDrop={() => handleDrop(col.name)}
                    className={`border-b hover:bg-muted/50 cursor-move transition-colors ${
                      draggedColumn === col.name ? 'opacity-50' : ''
                    } ${
                      dragOverColumn === col.name && draggedColumn !== col.name
                        ? isDraggingDown
                          ? 'border-b-2 border-b-primary bg-accent/50'
                          : 'border-t-2 border-t-primary bg-accent/50'
                        : ''
                    }`}
                  >
                    <td className="py-2 px-2">
                      <GripVertical className="w-4 h-4 text-muted-foreground" />
                    </td>
                    <td className="py-2 px-2">
                      <input
                        type="checkbox"
                        checked={col.visible}
                        onChange={(e) => {
                          setColumns((prev) =>
                            prev.map((c) =>
                              c.name === col.name ? { ...c, visible: e.target.checked } : c
                            )
                          )
                        }}
                      />
                    </td>
                    <td className="py-2 px-2 truncate">{col.display_name}</td>
                    <td className="py-2 px-2 text-center">
                      <button
                        onClick={() => {
                          setColumns((prev) =>
                            prev.map((c) => {
                              if (c.name === col.name) {
                                const nextSort =
                                  c.sort_direction === 'none'
                                    ? 'asc'
                                    : c.sort_direction === 'asc'
                                    ? 'desc'
                                    : 'none'
                                return { ...c, sort_direction: nextSort }
                              } else {
                                return { ...c, sort_direction: 'none' }
                              }
                            })
                          )
                          setCurrentPage(1)
                        }}
                        className="w-6 h-6 flex items-center justify-center text-xs rounded hover:bg-muted mx-auto"
                        title={`Sort: ${col.sort_direction}`}
                      >
                        {col.sort_direction === 'asc' ? (
                          <span>↑</span>
                        ) : col.sort_direction === 'desc' ? (
                          <span>↓</span>
                        ) : (
                          <span className="text-muted-foreground">⇅</span>
                        )}
                      </button>
                    </td>
                    <td className="py-2 px-2">
                      {filters.some((f) => f.column_name === col.name) ? (
                        <Badge
                          variant="secondary"
                          className="flex items-center gap-1 max-w-full cursor-pointer hover:bg-secondary/80"
                          onClick={() => setFilterModalColumn(col)}
                          title={filters.find((f) => f.column_name === col.name)?.filter_value}
                        >
                          <span className="truncate text-xs">
                            {filters.find((f) => f.column_name === col.name)?.filter_value}
                          </span>
                          <button
                            onClick={(e) => {
                              e.stopPropagation()
                              handleRemoveFilter(col.name)
                            }}
                            className="ml-0.5 hover:bg-secondary-foreground/20 rounded-sm"
                            title="Remove filter"
                          >
                            <X className="h-3 w-3" />
                          </button>
                        </Badge>
                      ) : (
                        <button
                          onClick={() => setFilterModalColumn(col)}
                          className="w-6 h-6 flex items-center justify-center rounded hover:bg-muted mx-auto text-muted-foreground hover:text-foreground transition-colors"
                          title="Add filter"
                        >
                          <Plus className="h-4 w-4" />
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
            </tbody>
          </table>
        </CardContent>
      </Card>

      {/* Data Table Card - Right Panel */}
      <Card className="flex-1 flex flex-col">
        <CardContent className="flex-1 overflow-auto p-0">
          {loading ? (
            <div className="p-4">Loading data...</div>
          ) : dataRows.length > 0 ? (
            <div className="flex flex-col h-full">
              <div className="flex-1 overflow-auto">
                <table className="w-full border-collapse">
                  <thead className="bg-muted sticky top-0">
                    <tr>
                      {dataColumns.map((colName) => {
                        const colMeta = columns.find((c) => c.name === colName)
                        const displayName = colMeta ? colMeta.display_name : colName

                        return (
                          <th
                            key={colName}
                            className="border border-border px-4 py-2 font-medium text-center uppercase text-xs tracking-wide"
                          >
                            {displayName}
                          </th>
                        )
                      })}
                    </tr>
                  </thead>
                  <tbody>
                    {dataRows.map((row, rowIndex) => (
                      <tr key={rowIndex} className="hover:bg-muted/50">
                        {row.map((cell, cellIndex) => {
                          const colName = dataColumns[cellIndex]
                          const colMeta = columns.find((c) => c.name === colName)
                          const alignClass = colMeta
                            ? colMeta.alignment === 'Right'
                              ? 'text-right'
                              : colMeta.alignment === 'Center'
                              ? 'text-center'
                              : 'text-left'
                            : 'text-left'

                          // Format date columns in user's local timezone
                          // Backend sends "-" for null dates, don't try to parse it
                          const displayValue = colMeta?.col_type === 'Date' && cell !== '-'
                            ? formatDateFull(parseInt(cell))
                            : cell

                          return (
                            <td
                              key={cellIndex}
                              className={`border border-border px-4 py-2 ${alignClass}`}
                            >
                              {displayValue}
                            </td>
                          )
                        })}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>

              {/* Pagination */}
              <div className="flex items-center justify-between p-4 border-t border-border">
                <div className="text-sm text-muted-foreground">
                  Showing {(currentPage - 1) * ITEMS_PER_PAGE + 1} to{' '}
                  {Math.min(currentPage * ITEMS_PER_PAGE, totalCount)} of {totalCount}
                </div>
                <div className="flex items-center gap-2">
                  <button
                    onClick={() => currentPage > 1 && setCurrentPage(p => p - 1)}
                    disabled={currentPage === 1}
                    className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
                  >
                    Previous
                  </button>
                  <button
                    onClick={() => currentPage * ITEMS_PER_PAGE < totalCount && setCurrentPage(p => p + 1)}
                    disabled={currentPage * ITEMS_PER_PAGE >= totalCount}
                    className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
                  >
                    Next
                  </button>
                </div>
              </div>
            </div>
          ) : null}
        </CardContent>
      </Card>

      {/* Filter Modal */}
      <FilterModal
        column={filterModalColumn}
        domain={domain}
        open={filterModalColumn !== null}
        onClose={() => setFilterModalColumn(null)}
        onApply={handleApplyFilter}
      />
    </div>
  )
}
