import { useState, useEffect, useCallback, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import { fetchQuery, countQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import type { FlatTreeItem } from '@/hooks/useVirtualTree'

const ITEMS_PER_PAGE = 100

interface SearchResultsListProps {
  rootId: number
  searchQuery: string
  showTombstones: boolean
}

/**
 * Displays search results as a flat, paginated list.
 * Items are rendered using the same TreeNode component but without expand/collapse.
 */
export function SearchResultsList({ rootId, searchQuery, showTombstones }: SearchResultsListProps) {
  const [items, setItems] = useState<FlatTreeItem[]>([])
  const [totalCount, setTotalCount] = useState(0)
  const [currentPage, setCurrentPage] = useState(1)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const parentRef = useRef<HTMLDivElement>(null)

  // Track when we need to re-count
  const lastQueryRef = useRef<string>('')

  // Load search results
  const loadResults = useCallback(async () => {
    if (!searchQuery.trim()) {
      setItems([])
      setTotalCount(0)
      return
    }

    setLoading(true)
    setError(null)

    try {
      const columns: ColumnSpec[] = [
        { name: 'item_id', visible: true, sort_direction: 'none', position: 0 },
        { name: 'item_path', visible: true, sort_direction: 'asc', position: 1 },
        { name: 'item_type', visible: true, sort_direction: 'none', position: 2 },
        { name: 'is_ts', visible: true, sort_direction: 'none', position: 3 },
      ]

      // Build filters
      // Note: String values need single quotes, % wildcards are added by query builder
      const filters = [
        { column: 'root_id', value: rootId.toString() },
        { column: 'item_path', value: `'${searchQuery}'` },
      ]

      // Filter out tombstones if not showing them
      if (!showTombstones) {
        filters.push({ column: 'is_ts', value: 'F' })
      }

      // Build query key to detect changes
      const queryKey = JSON.stringify({ searchQuery, rootId, showTombstones })
      const needsCount = queryKey !== lastQueryRef.current

      // Get count when query changes
      if (needsCount) {
        const countData = await countQuery('items', {
          columns,
          filters,
        })
        setTotalCount(countData.count)
        lastQueryRef.current = queryKey

        // Reset to page 1 when query changes
        if (currentPage !== 1) {
          setCurrentPage(1)
          return // loadResults will be called again due to currentPage change
        }
      }

      // Fetch current page
      const response = await fetchQuery('items', {
        columns,
        filters,
        limit: ITEMS_PER_PAGE,
        offset: (currentPage - 1) * ITEMS_PER_PAGE,
      })

      // Transform response to FlatTreeItem format
      const flatItems: FlatTreeItem[] = response.rows.map(row => {
        const itemPath = row[1]
        const itemType = row[2] as 'F' | 'D' | 'S' | 'O'

        return {
          item_id: parseInt(row[0]),
          item_path: itemPath,
          item_name: itemPath.split('/').filter(Boolean).pop() || itemPath,
          item_type: itemType,
          is_ts: row[3] === '1' || row[3] === 'true',
          depth: 0, // Flat list, no depth
          isExpanded: false,
          childrenLoaded: false,
          hasChildren: itemType === 'D',
        }
      })

      setItems(flatItems)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to search items')
      console.error('Error searching items:', err)
    } finally {
      setLoading(false)
    }
  }, [rootId, searchQuery, showTombstones, currentPage])

  // Load results when dependencies change
  useEffect(() => {
    loadResults()
  }, [loadResults])

  // TanStack Virtual virtualizer
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,
    overscan: 5,
  })

  const totalPages = Math.ceil(totalCount / ITEMS_PER_PAGE)

  if (loading && items.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        Searching...
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64 text-red-600">
        {error}
      </div>
    )
  }

  if (items.length === 0 && !loading) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        No items found matching "{searchQuery}"
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      {/* Results count and pagination info */}
      <div className="flex items-center justify-between px-4 py-2 text-sm text-muted-foreground border-b">
        <span>
          {totalCount} result{totalCount !== 1 ? 's' : ''} found
        </span>
        {totalPages > 1 && (
          <span>
            Page {currentPage} of {totalPages}
          </span>
        )}
      </div>

      {/* Virtualized results list */}
      <div
        ref={parentRef}
        className="p-4 overflow-auto flex-1"
        style={{ height: '556px' }} // 600px - header height
      >
        <div
          style={{
            height: `${virtualizer.getTotalSize()}px`,
            width: '100%',
            position: 'relative',
          }}
        >
          {virtualizer.getVirtualItems().map(virtualItem => {
            const item = items[virtualItem.index]
            return (
              <div
                key={item.item_id}
                style={{
                  position: 'absolute',
                  top: 0,
                  left: 0,
                  width: '100%',
                  height: `${virtualItem.size}px`,
                  transform: `translateY(${virtualItem.start}px)`,
                }}
              >
                <TreeNode
                  item={item}
                  rootId={rootId}
                  expandable={false}
                  showPathTooltip={true}
                />
              </div>
            )
          })}
        </div>
      </div>

      {/* Pagination controls */}
      {totalPages > 1 && (
        <div className="flex items-center justify-center gap-2 px-4 py-3 border-t">
          <button
            onClick={() => setCurrentPage(p => Math.max(1, p - 1))}
            disabled={currentPage === 1 || loading}
            className="px-3 py-1 text-sm border rounded hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Previous
          </button>
          <span className="text-sm text-muted-foreground">
            {currentPage} / {totalPages}
          </span>
          <button
            onClick={() => setCurrentPage(p => Math.min(totalPages, p + 1))}
            disabled={currentPage === totalPages || loading}
            className="px-3 py-1 text-sm border rounded hover:bg-accent disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Next
          </button>
        </div>
      )}
    </div>
  )
}
