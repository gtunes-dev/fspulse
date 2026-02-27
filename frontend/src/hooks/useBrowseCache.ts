import { useState, useCallback, useRef, useEffect } from 'react'
import type { ChangeKind } from '@/lib/pathUtils'

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
  size: number | null
  mod_date: number | null
  change_kind: ChangeKind
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  unchanged_count: number | null
}

export interface BrowseCache {
  getChildren(parentPath: string): CachedItem[] | undefined
  loadChildren(parentPath: string): Promise<CachedItem[]>
  isPathLoading(parentPath: string): boolean
}

/**
 * Shared data cache for Browse page views.
 *
 * Both Tree and Folder views call loadChildren() instead of fetching directly.
 * The cache deduplicates concurrent requests and serves instant cache hits
 * when both views access the same paths.
 *
 * Resets automatically when rootId or scanId change.
 */
export function useBrowseCache(rootId: number, scanId: number): BrowseCache {
  const cacheRef = useRef<Map<string, CachedItem[]>>(new Map())
  const inflightRef = useRef<Map<string, Promise<CachedItem[]>>>(new Map())
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set())

  // Capture current rootId/scanId for stale-check inside async closures
  const contextRef = useRef({ rootId, scanId })
  contextRef.current = { rootId, scanId }

  // Clear cache refs synchronously during render when root/scan changes.
  // This prevents a race where child effects call loadChildren() before
  // the cleanup useEffect has fired, returning stale cached data.
  const prevContextRef = useRef({ rootId, scanId })
  if (prevContextRef.current.rootId !== rootId || prevContextRef.current.scanId !== scanId) {
    cacheRef.current.clear()
    inflightRef.current.clear()
    prevContextRef.current = { rootId, scanId }
  }

  // Reset loading state after render (state updates can't happen during render)
  useEffect(() => {
    setLoadingPaths(new Set())
  }, [rootId, scanId])

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
    const capturedRootId = rootId
    const capturedScanId = scanId

    const promise = (async () => {
      setLoadingPaths(prev => {
        const next = new Set(prev)
        next.add(parentPath)
        return next
      })

      try {
        const params = new URLSearchParams({
          root_id: capturedRootId.toString(),
          parent_path: parentPath,
          scan_id: capturedScanId.toString(),
        })

        const response = await fetch(`/api/items/immediate-children?${params}`)
        if (!response.ok) {
          throw new Error(`Failed to fetch children: ${response.statusText}`)
        }

        const data = await response.json() as Array<{
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

        const items: CachedItem[] = data.map(item => {
          const change_kind: ChangeKind = item.is_deleted ? 'deleted'
            : item.first_scan_id === capturedScanId && item.is_added ? 'added'
            : item.first_scan_id === capturedScanId ? 'modified'
            : 'unchanged'

          // Unchanged directories: derive counts from the temporal version.
          // No descendants changed, so adds/mods/dels are 0 and everyone
          // previously alive is now unchanged.
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
          }
        })

        // Guard against stale writes after root/scan change
        if (contextRef.current.rootId === capturedRootId &&
            contextRef.current.scanId === capturedScanId) {
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
  }, [rootId, scanId])

  const isPathLoading = useCallback((parentPath: string): boolean => {
    return loadingPaths.has(parentPath)
  }, [loadingPaths])

  return { getChildren, loadChildren, isPathLoading }
}
