import { useState, useEffect, useRef, forwardRef, useImperativeHandle } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import type { TreeNodeData } from '@/lib/pathUtils'
import { sortTreeItems } from '@/lib/pathUtils'
import { useVirtualTree } from '@/hooks/useVirtualTree'
import type { BrowseCache } from '@/hooks/useBrowseCache'
import { useScrollElement, useScrollMargin } from '@/contexts/ScrollContext'

interface FileTreeViewProps {
  rootPath: string
  scanId: number
  cache: BrowseCache
  showDeleted: boolean
  isActive?: boolean
  selectedItemId?: number | null
  onItemSelect?: (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => void
}

export interface FileTreeViewHandle {
  revealPath(targetPath: string): Promise<void>
}

export const FileTreeView = forwardRef<FileTreeViewHandle, FileTreeViewProps>(
  function FileTreeView({ rootPath, scanId, cache, showDeleted, isActive = true, selectedItemId, onItemSelect }, ref) {
    const [loading, setLoading] = useState(false)
    const [error, setError] = useState<string | null>(null)

    const parentRef = useRef<HTMLDivElement>(null)
    const scrollElement = useScrollElement()

    // Track what we've loaded to prevent duplicate fetches
    const loadedKeyRef = useRef<string | null>(null)

    // Virtual tree hook using the shared cache
    const { flatItems, initializeTree, toggleNode, isLoading: isNodeLoading, revealPath: treeRevealPath } = useVirtualTree({
      loadChildrenFn: cache.loadChildren,
    })

    // Filter deleted items client-side based on showDeleted toggle
    const visibleItems = showDeleted
      ? flatItems
      : flatItems.filter(item => !item.is_deleted)

    const scrollMargin = useScrollMargin(parentRef)

    // TanStack Virtual virtualizer — uses <main> as scroll element
    const virtualizer = useVirtualizer({
      count: visibleItems.length,
      getScrollElement: () => isActive ? scrollElement : null,
      estimateSize: () => 36,
      scrollMargin,
      overscan: 5,
    })

    // Scroll-to-target after revealPath
    const [pendingScrollTarget, setPendingScrollTarget] = useState<number | null>(null)

    useEffect(() => {
      if (pendingScrollTarget == null) return
      const index = visibleItems.findIndex(i => i.item_id === pendingScrollTarget)
      if (index !== -1) {
        virtualizer.scrollToIndex(index, { align: 'center' })
      }
      setPendingScrollTarget(null)
    }, [pendingScrollTarget, visibleItems, virtualizer])

    // Guard against concurrent reveals
    const revealInProgressRef = useRef(false)

    useImperativeHandle(ref, () => ({
      async revealPath(targetPath: string) {
        if (revealInProgressRef.current) return
        revealInProgressRef.current = true
        try {
          const itemId = await treeRevealPath(targetPath, rootPath)
          if (itemId != null) {
            setPendingScrollTarget(itemId)
          }
        } finally {
          revealInProgressRef.current = false
        }
      },
    }), [treeRevealPath, rootPath])

    useEffect(() => {
      if (!isActive) return

      // Create a unique key for this root+scan combination
      const loadKey = `${rootPath}:${scanId}`

      // Skip if we've already started loading this combination
      if (loadedKeyRef.current === loadKey) {
        return
      }

      // Mark as loading IMMEDIATELY to prevent Strict Mode duplicates
      loadedKeyRef.current = loadKey

      async function loadRootLevelItems() {
        setLoading(true)
        setError(null)

        try {
          const items = await cache.loadChildren(rootPath)

          // Transform to TreeNodeData
          const nodes: TreeNodeData[] = items.map(item => ({
            item_id: item.item_id,
            item_path: item.item_path,
            item_name: item.item_name,
            item_type: item.item_type,
            is_deleted: item.is_deleted,
            size: item.size,
            mod_date: item.mod_date,
            name: item.item_name,
          }))

          const sortedNodes = sortTreeItems(nodes)

          // Initialize virtual tree with root nodes
          initializeTree(sortedNodes)
        } catch (err) {
          // Reset on error to allow retry
          loadedKeyRef.current = null
          setError(err instanceof Error ? err.message : 'Failed to load items')
          console.error('Error loading root items:', err)
        } finally {
          setLoading(false)
        }
      }

      loadRootLevelItems()
    }, [isActive, rootPath, scanId, cache, initializeTree])

    return (
      <div
        ref={parentRef}
        className="p-4"
      >
        {loading ? (
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            Loading file tree...
          </div>
        ) : error ? (
          <div className="flex items-center justify-center h-32 text-red-600">
            {error}
          </div>
        ) : visibleItems.length === 0 ? (
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            No items found in this root
          </div>
        ) : (
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map(virtualItem => {
              const item = visibleItems[virtualItem.index]
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
                  <TreeNode
                    item={item}
                    onToggle={toggleNode}
                    isLoading={isNodeLoading(item.item_id)}
                    onItemSelect={onItemSelect}
                    isSelected={selectedItemId === item.item_id}
                  />
                </div>
              )
            })}
          </div>
        )}
      </div>
    )
  }
)
