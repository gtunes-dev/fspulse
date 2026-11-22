import { useState, useCallback, useRef, useEffect } from 'react'
import {
  describeTimelineItem,
  changeCounts,
  hashStateFromInt,
  valStateFromInt,
  type ChangeKind,
  type Happened,
  type HashState,
  type TimelineChange,
  type ValState,
} from '@/lib/pathUtils'
import { rangeKey, rangeParams, type TimeRangeSelection } from '@/lib/timeRange'

/**
 * Raw item data cached by parent path.
 * Stored unsorted — each consuming view applies its own sort.
 */
export interface CachedItem {
  item_id: number
  item_path: string
  item_name: string
  item_type: 'F' | 'D' | 'S' | 'O'
  is_deleted: boolean
  /** When tracking stopped because of an ignore pattern; null if tracked. */
  ignored_since: number | null
  /** Ignored at the start of the range, tracked again at the end. */
  resumed: boolean
  size: number | null
  mod_date: number | null
  change_kind: ChangeKind
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  unchanged_count: number | null
  hash_state: HashState | null
  val_state: ValState | null
}

export interface BrowseCache {
  getChildren(parentPath: string): CachedItem[] | undefined
  loadChildren(parentPath: string): Promise<CachedItem[]>
  isPathLoading(parentPath: string): boolean
}

/** One child of a folder as `/api/timeline/children` returns it. */
interface TimelineChild {
  item_id: number
  item_path: string
  item_name: string
  item_type: string
  is_deleted: boolean
  /** Not tracked because of an ignore pattern; `first_seen_at` is since when. */
  is_ignored: boolean
  first_seen_at: number
  size: number | null
  mod_date: number | null
  val_state: number | null
  hash_state: number | null
  /** Absent for a point in time. */
  change: TimelineChange | null
  /** What happened to the item inside the range. Absent for a point in time. */
  happened: Happened | null
  /** Folders in a range: how many items beneath the folder each kind of change happened to. */
  descendants: { added: number; modified: number; deleted: number; transient: number } | null
}

/**
 * Shared data cache for Browse page views.
 *
 * Both Tree and Folder views call loadChildren() instead of fetching directly.
 * The cache deduplicates concurrent requests and serves instant cache hits
 * when both views access the same paths.
 *
 * Resets automatically when the root, the time selection, the refresh
 * token, or whether ignored items are shown change.
 */
export function useBrowseCache(
  rootId: number,
  selection: TimeRangeSelection,
  refreshToken: number,
  includeIgnored: boolean
): BrowseCache {
  const cacheRef = useRef<Map<string, CachedItem[]>>(new Map())
  const inflightRef = useRef<Map<string, Promise<CachedItem[]>>>(new Map())
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set())

  // Everything that decides what a folder's children are.
  const contextKey = `${rootId}:${rangeKey(selection)}:${refreshToken}:${includeIgnored}`

  // Capture the current context for stale-checks inside async closures
  const contextRef = useRef(contextKey)
  contextRef.current = contextKey

  // Clear cache refs synchronously during render when the context changes.
  // This prevents a race where child effects call loadChildren() before
  // the cleanup useEffect has fired, returning stale cached data.
  const prevContextRef = useRef(contextKey)
  if (prevContextRef.current !== contextKey) {
    cacheRef.current.clear()
    inflightRef.current.clear()
    prevContextRef.current = contextKey
  }

  // Reset loading state after render (state updates can't happen during render)
  useEffect(() => {
    setLoadingPaths(new Set())
  }, [contextKey])

  const getChildren = useCallback((parentPath: string): CachedItem[] | undefined => {
    return cacheRef.current.get(parentPath)
  }, [])

  const loadChildren = useCallback(async (parentPath: string): Promise<CachedItem[]> => {
    // Cache hit
    const cached = cacheRef.current.get(parentPath)
    if (cached) return cached

    // In-flight dedup — return existing promise
    const inflight = inflightRef.current.get(parentPath)
    if (inflight) return inflight

    // New fetch
    const capturedContext = contextKey

    const promise = (async () => {
      setLoadingPaths(prev => {
        const next = new Set(prev)
        next.add(parentPath)
        return next
      })

      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          parent_path: parentPath,
          ...rangeParams(selection),
          ...(includeIgnored ? { include_ignored: 'true' } : {}),
        })

        const response = await fetch(`/api/timeline/children?${params}`)
        if (!response.ok) {
          throw new Error(`Failed to fetch children: ${response.statusText}`)
        }

        const data = await response.json() as { items: TimelineChild[] }

        const items: CachedItem[] = data.items.map(item => {
          return {
            item_id: item.item_id,
            item_path: item.item_path,
            item_name: item.item_name,
            item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
            is_deleted: item.is_deleted,
            size: item.size,
            mod_date: item.mod_date,
            ...describeTimelineItem(item),
            ...changeCounts(item),
            hash_state: hashStateFromInt(item.hash_state),
            val_state: valStateFromInt(item.val_state),
          }
        })

        // Guard against stale writes after the context changed
        if (contextRef.current === capturedContext) {
          cacheRef.current.set(parentPath, items)
        }

        return items
      } finally {
        inflightRef.current.delete(parentPath)
        setLoadingPaths(prev => {
          const next = new Set(prev)
          next.delete(parentPath)
          return next
        })
      }
    })()

    inflightRef.current.set(parentPath, promise)
    return promise
    // contextKey stands in for rootId, selection, refreshToken and includeIgnored.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [contextKey])

  const isPathLoading = useCallback((parentPath: string): boolean => {
    return loadingPaths.has(parentPath)
  }, [loadingPaths])

  return { getChildren, loadChildren, isPathLoading }
}
