import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { TreeNode } from './TreeNode'
import { fetchQuery } from '@/lib/api'
import { useScanManager } from '@/contexts/ScanManagerContext'
import type { ColumnSpec } from '@/lib/types'
import type { TreeNodeData } from '@/lib/pathUtils'
import { sortTreeItems } from '@/lib/pathUtils'
import { useVirtualTree } from '@/hooks/useVirtualTree'

interface FileTreeViewProps {
  rootId: number
  rootPath: string
  showTombstones: boolean
}

export function FileTreeView({ rootId, rootPath, showTombstones }: FileTreeViewProps) {
  const { activeScan } = useScanManager()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const parentRef = useRef<HTMLDivElement>(null)

  // Track what we've loaded to prevent duplicate fetches
  const loadedRootRef = useRef<string | null>(null)

  // Check if this root is currently being scanned
  const isRootBeingScanned = activeScan?.root_path === rootPath

  // Virtual tree hook - NO allItems, always load on demand
  const { flatItems, initializeTree, toggleNode, isLoading: isNodeLoading } = useVirtualTree({
    rootId,
  })

  // Filter tombstones client-side based on showTombstones toggle
  const visibleItems = showTombstones
    ? flatItems
    : flatItems.filter(item => !item.is_ts)

  // TanStack Virtual virtualizer
  const virtualizer = useVirtualizer({
    count: visibleItems.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36, // Fixed row height in pixels
    overscan: 5,
  })

  useEffect(() => {
    // Don't load items if the root is currently being scanned
    if (isRootBeingScanned) {
      loadedRootRef.current = null // Reset so we reload when scan finishes
      initializeTree([])
      return
    }

    // Create a unique key for this root
    const rootKey = `${rootId}:${rootPath}`

    // Skip if we've already started loading this root
    if (loadedRootRef.current === rootKey) {
      return
    }

    // Mark as loading IMMEDIATELY to prevent Strict Mode duplicates
    loadedRootRef.current = rootKey

    async function loadRootLevelItems() {
      setLoading(true)
      setError(null)

      try {
        // First, check if there are any completed scans for this root
        const scanColumns: ColumnSpec[] = [
          { name: 'scan_id', visible: true, sort_direction: 'desc', position: 0 }
        ]

        const scanResponse = await fetchQuery('scans', {
          columns: scanColumns,
          filters: [
            { column: 'root_id', value: rootId.toString() },
            { column: 'scan_state', value: 'C' }, // Completed scans only
          ],
          limit: 1,
          offset: 0,
        })

        if (scanResponse.rows.length === 0) {
          setError('No completed scans found for this root')
          initializeTree([])
          return
        }

        // Load ONLY root-level items using the new endpoint
        // Backend always returns tombstones - we filter client-side
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          parent_path: rootPath,
        })

        const response = await fetch(`/api/items/immediate-children?${params}`)
        if (!response.ok) {
          throw new Error(`Failed to fetch root items: ${response.statusText}`)
        }

        const items = await response.json() as Array<{
          item_id: number
          item_path: string
          item_type: string
          is_ts: boolean
        }>

        // Transform to TreeNodeData
        const nodes: TreeNodeData[] = items.map(item => {
          const itemName = item.item_path.split('/').filter(Boolean).pop() || item.item_path
          return {
            item_id: item.item_id,
            item_path: item.item_path,
            item_name: itemName,
            item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
            is_ts: item.is_ts,
            name: itemName,
          }
        })

        const sortedNodes = sortTreeItems(nodes)

        // Initialize virtual tree with root nodes
        initializeTree(sortedNodes)
      } catch (err) {
        // Reset on error to allow retry
        loadedRootRef.current = null
        setError(err instanceof Error ? err.message : 'Failed to load items')
        console.error('Error loading root items:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRootLevelItems()
  }, [rootId, rootPath, isRootBeingScanned, initializeTree])

  // Show message if root is currently being scanned
  if (isRootBeingScanned) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        <div className="text-center">
          <p className="text-lg mb-2">Scan in progress</p>
          <p className="text-sm">Browse is unavailable while this root is being scanned</p>
        </div>
      </div>
    )
  }

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
