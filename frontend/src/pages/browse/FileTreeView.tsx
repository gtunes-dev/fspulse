import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import type { TreeNodeData } from '@/lib/pathUtils'
import { sortTreeItems } from '@/lib/pathUtils'
import { useVirtualTree } from '@/hooks/useVirtualTree'

interface FileTreeViewProps {
  rootId: number
  rootPath: string
  scanId: number
  showDeleted: boolean
}

export function FileTreeView({ rootId, rootPath, scanId, showDeleted }: FileTreeViewProps) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const parentRef = useRef<HTMLDivElement>(null)

  // Track what we've loaded to prevent duplicate fetches
  const loadedKeyRef = useRef<string | null>(null)

  // Virtual tree hook with temporal scan_id
  const { flatItems, initializeTree, toggleNode, isLoading: isNodeLoading } = useVirtualTree({
    rootId,
    scanId,
  })

  // Filter deleted items client-side based on showDeleted toggle
  const visibleItems = showDeleted
    ? flatItems
    : flatItems.filter(item => !item.is_deleted)

  // TanStack Virtual virtualizer
  const virtualizer = useVirtualizer({
    count: visibleItems.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,
    overscan: 5,
  })

  useEffect(() => {
    // Create a unique key for this root+scan combination
    const loadKey = `${rootId}:${rootPath}:${scanId}`

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
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          parent_path: rootPath,
          scan_id: scanId.toString(),
        })

        const response = await fetch(`/api/items/immediate-children?${params}`)
        if (!response.ok) {
          throw new Error(`Failed to fetch root items: ${response.statusText}`)
        }

        const items = await response.json() as Array<{
          item_id: number
          item_path: string
          item_name: string
          item_type: string
          is_deleted: boolean
        }>

        // Transform to TreeNodeData
        const nodes: TreeNodeData[] = items.map(item => ({
          item_id: item.item_id,
          item_path: item.item_path,
          item_name: item.item_name,
          item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
          is_ts: item.is_deleted,
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
  }, [rootId, rootPath, scanId, initializeTree])

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        Loading file tree...
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

  if (visibleItems.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        No items found in this root
      </div>
    )
  }

  return (
    <div
      ref={parentRef}
      className="border border-border rounded-lg p-4 overflow-auto"
      style={{ height: '600px' }}
    >
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
                transform: `translateY(${virtualItem.start}px)`,
              }}
            >
              <TreeNode
                item={item}
                rootId={rootId}
                onToggle={toggleNode}
                isLoading={isNodeLoading(item.item_id)}
              />
            </div>
          )
        })}
      </div>
    </div>
  )
}
