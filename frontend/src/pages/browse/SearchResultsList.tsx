import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import type { FlatTreeItem } from '@/lib/pathUtils'

interface SearchResultsListProps {
  rootId: number
  rootPath: string
  scanId: number
  searchQuery: string
  showDeleted: boolean
  isActive?: boolean
  selectedItemId?: number | null
  onItemSelect?: (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => void
}

export function SearchResultsList({
  rootId,
  rootPath,
  scanId,
  searchQuery,
  showDeleted,
  isActive = true,
  selectedItemId,
  onItemSelect,
}: SearchResultsListProps) {
  const [results, setResults] = useState<FlatTreeItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const parentRef = useRef<HTMLDivElement>(null)

  // Track last request to avoid stale responses
  const requestIdRef = useRef(0)

  const lastFetchKeyRef = useRef<string | null>(null)
  useEffect(() => {
    if (!isActive) return

    const fetchKey = `${rootId}:${scanId}:${searchQuery}`
    if (lastFetchKeyRef.current === fetchKey) return
    lastFetchKeyRef.current = fetchKey

    const currentRequestId = ++requestIdRef.current

    async function search() {
      setLoading(true)
      setError(null)

      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          scan_id: scanId.toString(),
          query: searchQuery,
        })

        const response = await fetch(`/api/items/search?${params}`)
        if (currentRequestId !== requestIdRef.current) return

        if (!response.ok) {
          throw new Error(`Search failed: ${response.statusText}`)
        }

        const items = (await response.json()) as Array<{
          item_id: number
          item_path: string
          item_name: string
          item_type: string
          is_deleted: boolean
          size: number | null
          mod_date: number | null
        }>

        const flatItems: FlatTreeItem[] = items.map((item) => ({
          item_id: item.item_id,
          item_path: item.item_path,
          item_name: item.item_name,
          item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
          is_deleted: item.is_deleted,
          size: item.size,
          mod_date: item.mod_date,
          depth: 0,
          isExpanded: false,
          childrenLoaded: false,
          hasChildren: item.item_type === 'D',
        }))

        setResults(flatItems)
      } catch (err) {
        if (currentRequestId !== requestIdRef.current) return
        lastFetchKeyRef.current = null // Allow retry on error
        setError(err instanceof Error ? err.message : 'Search failed')
        console.error('Search error:', err)
      } finally {
        if (currentRequestId === requestIdRef.current) {
          setLoading(false)
        }
      }
    }

    search()
  }, [isActive, rootId, scanId, searchQuery])

  // Filter deleted items client-side
  const visibleResults = showDeleted
    ? results
    : results.filter((item) => !item.is_deleted)

  const virtualizer = useVirtualizer({
    count: visibleResults.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 52, // Taller rows to accommodate parent path
    overscan: 5,
  })

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        Searching...
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-full text-red-600">
        {error}
      </div>
    )
  }

  if (visibleResults.length === 0) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground">
        No results found
      </div>
    )
  }

  // Extract parent path relative to root for display
  const getParentPath = (itemPath: string) => {
    const sep = '/'
    const lastSepIdx = itemPath.lastIndexOf(sep)
    if (lastSepIdx <= 0) return sep
    const parentPath = itemPath.substring(0, lastSepIdx)
    // Make relative to root
    if (parentPath === rootPath || parentPath === rootPath.replace(/\/$/, '')) {
      return sep
    }
    const rootPrefix = rootPath.endsWith(sep) ? rootPath : rootPath + sep
    if (parentPath.startsWith(rootPrefix)) {
      return parentPath.substring(rootPrefix.length)
    }
    return parentPath
  }

  return (
    <div
      ref={parentRef}
      className="h-full p-4 overflow-auto"
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const item = visibleResults[virtualItem.index]
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
              <div className="flex flex-col">
                <TreeNode
                  item={item}
                  expandable={false}
                  showPathTooltip={true}
                  onItemSelect={onItemSelect}
                  isSelected={selectedItemId === item.item_id}
                />
                <span className="text-xs text-muted-foreground pl-14 -mt-1">
                  {getParentPath(item.item_path)}
                </span>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
