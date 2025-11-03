import { useState, useEffect } from 'react'
import { TreeNode } from './TreeNode'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import type { ItemData, TreeNodeData } from '@/lib/pathUtils'
import { getImmediateChildren, itemToTreeNode, sortTreeItems } from '@/lib/pathUtils'

interface FileTreeViewProps {
  rootId: number
  rootPath: string
  showTombstones: boolean
  searchFilter?: string
}

export function FileTreeView({ rootId, rootPath, showTombstones, searchFilter }: FileTreeViewProps) {
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [allItems, setAllItems] = useState<ItemData[]>([])
  const [rootLevelNodes, setRootLevelNodes] = useState<TreeNodeData[]>([])

  useEffect(() => {
    async function loadItems() {
      setLoading(true)
      setError(null)

      try {
        // First, get the latest completed scan for this root
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
          setAllItems([])
          setRootLevelNodes([])
          return
        }

        const latestScanId = scanResponse.rows[0][0]

        // Query all items from the latest scan
        const filters: Array<{ column: string; value: string }> = [
          { column: 'root_id', value: rootId.toString() },
          { column: 'last_scan', value: latestScanId },
        ]

        if (!showTombstones) {
          filters.push({ column: 'is_ts', value: 'false' })
        }

        // Add search filter if provided
        if (searchFilter && searchFilter.trim()) {
          filters.push({ column: 'item_path', value: `'${searchFilter.trim()}'` })
        }

        const columns: ColumnSpec[] = [
          { name: 'item_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'item_path', visible: true, sort_direction: 'asc', position: 1 },
          { name: 'item_path@name', visible: true, sort_direction: 'none', position: 2 },
          { name: 'item_type', visible: true, sort_direction: 'none', position: 3 },
          { name: 'is_ts', visible: true, sort_direction: 'none', position: 4 },
        ]

        const response = await fetchQuery('items', {
          columns,
          filters,
          limit: 50000, // Generous limit
          offset: 0,
        })

        // Transform response to ItemData
        const items: ItemData[] = response.rows.map(row => ({
          item_id: parseInt(row[0]),
          item_path: row[1],
          item_name: row[2],
          item_type: row[3] as 'F' | 'D' | 'S' | 'O',
          is_ts: row[4] === 'true',
        }))

        setAllItems(items)

        // Filter to root-level items (immediate children of root path)
        const rootLevelItems = getImmediateChildren(items, rootPath)

        // Transform to TreeNodeData and sort
        const nodes = rootLevelItems.map(itemToTreeNode)
        const sortedNodes = sortTreeItems(nodes)

        setRootLevelNodes(sortedNodes)
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load items')
        console.error('Error loading items:', err)
      } finally {
        setLoading(false)
      }
    }

    loadItems()
  }, [rootId, rootPath, showTombstones, searchFilter])

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

  if (rootLevelNodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-64 text-muted-foreground">
        No items found in this root
      </div>
    )
  }

  return (
    <div className="border border-border rounded-lg p-4 overflow-auto">
      {rootLevelNodes.map(node => (
        <TreeNode
          key={node.item_id}
          node={node}
          rootId={rootId}
          level={0}
          showTombstones={showTombstones}
          allItems={allItems}
        />
      ))}
    </div>
  )
}
