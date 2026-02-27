import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import type { FlatTreeItem, ChangeKind } from '@/lib/pathUtils'
import { useScrollElement, useScrollMargin } from '@/contexts/ScrollContext'

interface SearchResultsListProps {
  rootId: number
  rootPath: string
  scanId: number
  searchQuery: string
  hiddenKinds: Set<ChangeKind>
  isActive?: boolean
  selectedItemId?: number | null
  onItemSelect?: (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => void
}

export function SearchResultsList({
  rootId,
  rootPath,
  scanId,
  searchQuery,
  hiddenKinds,
  isActive = true,
  selectedItemId,
  onItemSelect,
}: SearchResultsListProps) {
  const [results, setResults] = useState<FlatTreeItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const parentRef = useRef<HTMLDivElement>(null)
  const scrollElement = useScrollElement()

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
          is_added: boolean
          is_deleted: boolean
          size: number | null
          mod_date: number | null
          first_scan_id: number
          add_count: number | null
          modify_count: number | null
          delete_count: number | null
          unchanged_count: number | null
        }>

        const flatItems: FlatTreeItem[] = items.map((item) => {
          const change_kind = item.is_deleted ? 'deleted' as const
            : item.first_scan_id === scanId && item.is_added ? 'added' as const
            : item.first_scan_id === scanId ? 'modified' as const
            : 'unchanged' as const

          const isUnchangedDir = change_kind === 'unchanged' && item.item_type === 'D'

          return {
            item_id: item.item_id,
            item_path: item.item_path,
            item_name: item.item_name,
            item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
            is_deleted: item.is_deleted,
            size: item.size,
            mod_date: item.mod_date,
            change_kind,
            add_count: isUnchangedDir ? 0 : item.add_count,
            modify_count: isUnchangedDir ? 0 : item.modify_count,
            delete_count: isUnchangedDir ? 0 : item.delete_count,
            unchanged_count: isUnchangedDir
              ? (item.add_count ?? 0) + (item.modify_count ?? 0) + (item.unchanged_count ?? 0)
              : item.unchanged_count,
            depth: 0,
            isExpanded: false,
            childrenLoaded: false,
            hasChildren: item.item_type === 'D',
          }
        })

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

  // Filter items client-side based on change kind toggles
  const visibleResults = results.filter((item) => !hiddenKinds.has(item.change_kind))

  const scrollMargin = useScrollMargin(parentRef)

  // TanStack Virtual virtualizer — uses <main> as scroll element
  const virtualizer = useVirtualizer({
    count: visibleResults.length,
    getScrollElement: () => isActive ? scrollElement : null,
    estimateSize: () => 52, // Taller rows to accommodate parent path
    scrollMargin,
    overscan: 5,
  })

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
      className="p-4"
    >
      {loading ? (
        <div className="flex items-center justify-center h-32 text-muted-foreground">
          Searching...
        </div>
      ) : error ? (
        <div className="flex items-center justify-center h-32 text-red-600">
          {error}
        </div>
      ) : visibleResults.length === 0 ? (
        <div className="flex items-center justify-center h-32 text-muted-foreground">
          No results found
        </div>
      ) : (
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
                  transform: `translateY(${virtualItem.start - scrollMargin}px)`,
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
      )}
    </div>
  )
}
