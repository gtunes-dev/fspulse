import { useState, useCallback, useRef, useEffect } from 'react'

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

  // Reset cache when root or scan changes
  useEffect(() => {
    cacheRef.current.clear()
    inflightRef.current.clear()
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
          is_deleted: boolean
          size: number | null
          mod_date: number | null
        }>

        const items: CachedItem[] = data.map(item => ({
          ...item,
          item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
        }))

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
