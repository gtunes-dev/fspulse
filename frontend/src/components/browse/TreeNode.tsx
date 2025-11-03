import { useState } from 'react'
import { ChevronRight, ChevronDown, Folder, FolderOpen, File } from 'lucide-react'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { ItemDetailSheet } from './ItemDetailSheet'
import type { TreeNodeData, ItemData } from '@/lib/pathUtils'
import { getImmediateChildren, itemToTreeNode, sortTreeItems } from '@/lib/pathUtils'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'

interface TreeNodeProps {
  node: TreeNodeData
  rootId: number
  level: number
  showTombstones: boolean
  allItems?: ItemData[] // Pre-loaded items for efficient filtering
}

export function TreeNode({ node, rootId, level, showTombstones, allItems }: TreeNodeProps) {
  const [isOpen, setIsOpen] = useState(false)
  const [children, setChildren] = useState<TreeNodeData[]>([])
  const [isLoading, setIsLoading] = useState(false)
  const [sheetOpen, setSheetOpen] = useState(false)

  const handleItemClick = () => {
    setSheetOpen(true)
  }

  // File node (leaf)
  if (node.item_type === 'F' || node.item_type === 'S' || node.item_type === 'O') {
    return (
      <>
        <div
          className={`flex items-center gap-2 p-2 hover:bg-accent rounded cursor-pointer ${
            node.is_ts ? 'text-muted-foreground' : ''
          }`}
          style={{ paddingLeft: `${level * 20 + 8}px` }}
          onClick={handleItemClick}
        >
          <File className="h-4 w-4 flex-shrink-0" />
          <span className={node.is_ts ? 'line-through' : ''}>{node.name}</span>
        </div>
        <ItemDetailSheet
          itemId={node.item_id}
          itemPath={node.item_path}
          itemType={node.item_type}
          isTombstone={node.is_ts}
          rootId={rootId}
          open={sheetOpen}
          onOpenChange={setSheetOpen}
        />
      </>
    )
  }

  // Folder node (collapsible)
  const handleToggle = async () => {
    const newOpenState = !isOpen

    // Load children on first expand
    if (newOpenState && children.length === 0) {
      setIsLoading(true)
      try {
        let childItems: ItemData[]

        if (allItems) {
          // Use pre-loaded items (more efficient)
          childItems = getImmediateChildren(allItems, node.item_path)
        } else {
          // Fallback: query from backend
          const filters: Array<{ column: string; value: string }> = [
            { column: 'root_id', value: rootId.toString() },
          ]

          if (!showTombstones) {
            filters.push({ column: 'is_ts', value: 'false' })
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
            limit: 10000,
            offset: 0,
          })

          childItems = response.rows.map(row => ({
            item_id: parseInt(row[0]),
            item_path: row[1],
            item_name: row[2],
            item_type: row[3] as 'F' | 'D' | 'S' | 'O',
            is_ts: row[4] === 'true',
          }))

          // Filter to immediate children
          childItems = getImmediateChildren(childItems, node.item_path)
        }

        // Transform to TreeNodeData and sort
        const childNodes = childItems.map(itemToTreeNode)
        const sortedChildren = sortTreeItems(childNodes)
        setChildren(sortedChildren)
      } catch (error) {
        console.error('Error loading children:', error)
      } finally {
        setIsLoading(false)
      }
    }

    setIsOpen(newOpenState)
  }

  return (
    <>
      <Collapsible open={isOpen} onOpenChange={handleToggle}>
        <div
          className={`flex items-center gap-2 p-2 hover:bg-accent rounded ${
            node.is_ts ? 'text-muted-foreground' : ''
          }`}
          style={{ paddingLeft: `${level * 20 + 8}px` }}
        >
          <CollapsibleTrigger className="flex items-center gap-2 flex-shrink-0">
            {isOpen ? (
              <ChevronDown className="h-4 w-4 flex-shrink-0" />
            ) : (
              <ChevronRight className="h-4 w-4 flex-shrink-0" />
            )}
            {isOpen ? (
              <FolderOpen className="h-4 w-4 flex-shrink-0" />
            ) : (
              <Folder className="h-4 w-4 flex-shrink-0" />
            )}
          </CollapsibleTrigger>
          <span
            className={`cursor-pointer ${node.is_ts ? 'line-through' : ''}`}
            onClick={handleItemClick}
          >
            {node.name}
          </span>
        </div>
      <CollapsibleContent>
        {isLoading ? (
          <div
            className="text-muted-foreground text-sm p-2"
            style={{ paddingLeft: `${(level + 1) * 20 + 8}px` }}
          >
            Loading...
          </div>
        ) : children.length === 0 ? (
          <div
            className="text-muted-foreground text-sm p-2"
            style={{ paddingLeft: `${(level + 1) * 20 + 8}px` }}
          >
            (empty)
          </div>
        ) : (
          <>
            {children.map(child => (
              <TreeNode
                key={child.item_id}
                node={child}
                rootId={rootId}
                level={level + 1}
                showTombstones={showTombstones}
                allItems={allItems}
              />
            ))}
          </>
        )}
      </CollapsibleContent>
    </Collapsible>
    <ItemDetailSheet
      itemId={node.item_id}
      itemPath={node.item_path}
      itemType={node.item_type}
      isTombstone={node.is_ts}
      rootId={rootId}
      open={sheetOpen}
      onOpenChange={setSheetOpen}
    />
  </>
  )
}
