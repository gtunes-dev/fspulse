import { useState } from 'react'
import { ChevronRight, ChevronDown, Folder, FolderOpen, File, Trash2 } from 'lucide-react'
import { ItemDetailSheet } from '@/components/shared/ItemDetailSheet'
import type { FlatTreeItem } from '@/lib/pathUtils'

interface TreeNodeProps {
  item: FlatTreeItem
  rootId: number
  onToggle?: (itemId: number) => void
  isLoading?: boolean
  /** When false, folders are not expandable (for flat search results) */
  expandable?: boolean
  /** When true, shows full path as tooltip (for search results) */
  showPathTooltip?: boolean
}

/**
 * Renders a single row in the virtualized tree or flat search results.
 * Can be either a file (leaf) or directory (collapsible when expandable=true).
 */
export function TreeNode({
  item,
  rootId,
  onToggle,
  isLoading = false,
  expandable = true,
  showPathTooltip = false
}: TreeNodeProps) {
  const [sheetOpen, setSheetOpen] = useState(false)

  const handleItemClick = () => {
    setSheetOpen(true)
  }

  const handleToggle = () => {
    if (item.hasChildren && onToggle && expandable) {
      onToggle(item.item_id)
    }
  }

  const paddingLeft = item.depth * 20 + 8

  // Shared styling for tombstones (deleted items)
  const tombstoneClass = item.is_deleted ? 'text-muted-foreground' : ''
  const textClass = item.is_deleted ? 'line-through' : ''

  // Render directory icon - expandable (button with chevron) or static (just folder icon)
  const DirectoryIcon = () => {
    if (!expandable) {
      // Static folder icon for flat search results
      return <Folder className="h-4 w-4 flex-shrink-0" />
    }

    // Expandable folder with chevron toggle
    return (
      <button
        onClick={handleToggle}
        className="flex items-center gap-2 flex-shrink-0 border-none bg-transparent p-0 cursor-pointer"
        disabled={isLoading}
        aria-label={item.isExpanded ? 'Collapse folder' : 'Expand folder'}
      >
        {isLoading ? (
          <div className="h-4 w-4 flex-shrink-0 animate-spin">⟳</div>
        ) : item.isExpanded ? (
          <ChevronDown className="h-4 w-4 flex-shrink-0" />
        ) : (
          <ChevronRight className="h-4 w-4 flex-shrink-0" />
        )}
        {item.isExpanded ? (
          <FolderOpen className="h-4 w-4 flex-shrink-0" />
        ) : (
          <Folder className="h-4 w-4 flex-shrink-0" />
        )}
      </button>
    )
  }

  // Render file icon
  const FileIcon = () => <File className="h-4 w-4 flex-shrink-0" />

  return (
    <>
      <div
        className={`flex items-center gap-2 p-2 hover:bg-accent rounded ${tombstoneClass}`}
        style={{ paddingLeft: `${paddingLeft}px` }}
      >
        {item.hasChildren ? <DirectoryIcon /> : <FileIcon />}
        <span
          className={`cursor-pointer ${textClass}`}
          onClick={handleItemClick}
          title={showPathTooltip ? item.item_path : undefined}
        >
          {item.item_name}
        </span>
        {item.is_deleted && (
          <Trash2
            className="h-4 w-4 flex-shrink-0 ml-2 text-muted-foreground"
            aria-label="Deleted item"
          />
        )}
      </div>
      <ItemDetailSheet
        itemId={item.item_id}
        itemPath={item.item_path}
        itemType={item.item_type}
        isTombstone={item.is_deleted}
        rootId={rootId}
        open={sheetOpen}
        onOpenChange={setSheetOpen}
      />
    </>
  )
}
