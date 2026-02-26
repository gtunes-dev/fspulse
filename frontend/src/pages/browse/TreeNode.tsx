import { ChevronRight, ChevronDown, Folder, FolderOpen, File, FileSymlink, FileQuestion, Trash2 } from 'lucide-react'
import { cn } from '@/lib/utils'
import type { FlatTreeItem } from '@/lib/pathUtils'

interface TreeNodeProps {
  item: FlatTreeItem
  onToggle?: (itemId: number) => void
  isLoading?: boolean
  /** When false, folders are not expandable (for flat search results) */
  expandable?: boolean
  /** When true, shows full path as tooltip (for search results) */
  showPathTooltip?: boolean
  /** Called when user clicks to select/inspect an item */
  onItemSelect?: (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => void
  /** Whether this node is the currently selected item */
  isSelected?: boolean
}

function getFileIcon(type: string, deleted: boolean) {
  const colorClass = deleted ? 'text-muted-foreground' : 'text-muted-foreground'
  switch (type) {
    case 'S': return <FileSymlink className={cn('h-4 w-4 flex-shrink-0', colorClass)} />
    case 'O': return <FileQuestion className={cn('h-4 w-4 flex-shrink-0', colorClass)} />
    default: return <File className={cn('h-4 w-4 flex-shrink-0', colorClass)} />
  }
}

/**
 * Renders a single row in the virtualized tree or flat search results.
 * Can be either a file (leaf) or directory (collapsible when expandable=true).
 */
export function TreeNode({
  item,
  onToggle,
  isLoading = false,
  expandable = true,
  showPathTooltip = false,
  onItemSelect,
  isSelected = false,
}: TreeNodeProps) {
  const handleItemClick = () => {
    onItemSelect?.({
      itemId: item.item_id,
      itemPath: item.item_path,
      itemType: item.item_type,
      isTombstone: item.is_deleted,
    })
  }

  const handleToggle = () => {
    if (item.hasChildren && onToggle && expandable) {
      onToggle(item.item_id)
    }
  }

  const paddingLeft = item.depth * 20 + 8
  const folderColor = item.is_deleted ? 'text-muted-foreground' : 'text-blue-500'

  // Render directory icon - expandable (button with chevron) or static (just folder icon)
  const DirectoryIcon = () => {
    if (!expandable) {
      return <Folder className={cn('h-4 w-4 flex-shrink-0', folderColor)} />
    }

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
          <FolderOpen className={cn('h-4 w-4 flex-shrink-0', folderColor)} />
        ) : (
          <Folder className={cn('h-4 w-4 flex-shrink-0', folderColor)} />
        )}
      </button>
    )
  }

  return (
    <div
      className={cn(
        'flex items-center gap-2 p-2 hover:bg-accent rounded',
        item.is_deleted && 'text-muted-foreground',
        isSelected && 'bg-accent',
      )}
      style={{ paddingLeft: `${paddingLeft}px` }}
    >
      {item.hasChildren ? (
        <DirectoryIcon />
      ) : (
        <>
          {expandable && <div className="w-4 flex-shrink-0" />}
          {getFileIcon(item.item_type, item.is_deleted)}
        </>
      )}
      {item.change_kind === 'added' && (
        <span className="inline-block w-1.5 h-1.5 rounded-full bg-green-500 flex-shrink-0" />
      )}
      {item.change_kind === 'modified' && (
        <span className="inline-block w-1.5 h-1.5 rounded-full bg-blue-500 flex-shrink-0" />
      )}
      <span
        className={cn('cursor-pointer', item.is_deleted && 'line-through')}
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
  )
}
