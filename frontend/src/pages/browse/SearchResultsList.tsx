import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import type { FlatTreeItem, ChangeKind, Happened, TimelineChange } from '@/lib/pathUtils'
import { changeCounts, describeTimelineItem, hashStateFromInt, valStateFromInt, isItemVisible } from '@/lib/pathUtils'
import { useScrollElement, useScrollMargin } from '@/contexts/ScrollContext'
import { rangeKey, rangeParams, type TimeRangeSelection } from '@/lib/timeRange'

interface SearchResultsListProps {
  rootId: number
  rootPath: string
  selection: TimeRangeSelection
  refreshToken: number
  /** Also find items that are not tracked because of an ignore pattern. */
  includeIgnored: boolean
  searchQuery: string
  hiddenKinds: Set<ChangeKind>
  isActive?: boolean
  selectedItemId?: number | null
  onItemSelect?: (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => void
}

export function SearchResultsList({
  rootId,
  rootPath,
  selection,
  refreshToken,
  includeIgnored,
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

  const selectionKey = rangeKey(selection)
  const lastFetchKeyRef = useRef<string | null>(null)
  useEffect(() => {
    if (!isActive) return

    const fetchKey = `${rootId}:${selectionKey}:${refreshToken}:${includeIgnored}:${searchQuery}`
    if (lastFetchKeyRef.current === fetchKey) return
    lastFetchKeyRef.current = fetchKey

    const currentRequestId = ++requestIdRef.current

    async function search() {
      setLoading(true)
      setError(null)

      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          query: searchQuery,
          limit: '2000',
          ...rangeParams(selection),
          ...(includeIgnored ? { include_ignored: 'true' } : {}),
        })

        const response = await fetch(`/api/timeline/search?${params}`)
        if (currentRequestId !== requestIdRef.current) return

        if (!response.ok) {
          throw new Error(`Search failed: ${response.statusText}`)
        }

        const data = (await response.json()) as {
          items: Array<{
            item_id: number
            item_path: string
            item_name: string
            item_type: string
            is_deleted: boolean
            is_ignored: boolean
            first_seen_at: number
            size: number | null
            mod_date: number | null
            val_state: number | null
            hash_state: number | null
            change: TimelineChange | null
            happened: Happened | null
          }>
        }

        const flatItems: FlatTreeItem[] = data.items.map((item) => ({
          item_id: item.item_id,
          item_path: item.item_path,
          item_name: item.item_name,
          item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
          is_deleted: item.is_deleted,
          size: item.size,
          mod_date: item.mod_date,
          ...describeTimelineItem(item),
          // Search results do not carry what is beneath a folder: its
          // square shows only what happened to the folder itself.
          ...changeCounts(item),
          hash_state: hashStateFromInt(item.hash_state),
          val_state: valStateFromInt(item.val_state),
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
    // selectionKey stands in for `selection`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isActive, rootId, selectionKey, refreshToken, includeIgnored, searchQuery])

  // Filter items client-side based on change kind, hash state, and val state toggles
  const visibleResults = results.filter((item) => isItemVisible(item, hiddenKinds))

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
      className="p-4 overflow-hidden"
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
                <div className="flex flex-col min-w-0 overflow-hidden">
                  <TreeNode
                    item={item}
                    expandable={false}
                    showPathTooltip={true}
                    onItemSelect={onItemSelect}
                    isSelected={selectedItemId === item.item_id}
                    hiddenKinds={hiddenKinds}
                  />
                  <span className="text-xs text-muted-foreground pl-14 -mt-1 truncate">
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
