// Path utility functions for file tree navigation

export interface ItemData {
  item_id: number
  item_path: string
  item_name: string // Filename/directory name (from backend using @name format)
  item_type: 'F' | 'D' | 'S' | 'O' // File, Directory, Symlink, Other
  is_ts: boolean // Is tombstone
}

/**
 * Represents a flattened tree item for virtualization.
 * Items are stored in a flat array with depth metadata for efficient rendering.
 */
export interface FlatTreeItem {
  item_id: number
  item_path: string
  item_name: string
  item_type: 'F' | 'D' | 'S' | 'O'
  is_deleted: boolean
  depth: number
  isExpanded: boolean
  childrenLoaded: boolean
  hasChildren: boolean
}

export interface TreeNodeData extends ItemData {
  name: string // Extracted from path (last segment)
  children?: TreeNodeData[] // Lazy-loaded
  isExpanded?: boolean
  isLoading?: boolean
}

/**
 * Get the parent path
 * Example: "/Users/alice/Documents/file.txt" -> "/Users/alice/Documents"
 * Example: "/Users/alice" -> "/Users"
 */
export function getParentPath(path: string): string {
  const segments = path.split('/').filter(Boolean)
  segments.pop()
  return segments.length > 0 ? '/' + segments.join('/') : '/'
}

/**
 * Check if a path is an immediate child of a parent path
 * Example: isImmediateChild("/a/b/c", "/a/b") -> true
 *          isImmediateChild("/a/b/c/d", "/a/b") -> false
 *          isImmediateChild("/a/b", "/a") -> true
 */
export function isImmediateChild(childPath: string, parentPath: string): boolean {
  // Normalize paths to not have trailing slashes
  const normalizedParent = parentPath.replace(/\/$/, '')
  const normalizedChild = childPath.replace(/\/$/, '')

  // Child must start with parent
  if (!normalizedChild.startsWith(normalizedParent)) return false

  // Get the part after the parent
  const relativePath = normalizedChild.substring(normalizedParent.length)

  // Remove leading slash if present
  const cleanRelative = relativePath.startsWith('/') ? relativePath.substring(1) : relativePath

  // Should have exactly one segment (no more slashes)
  const segments = cleanRelative.split('/').filter(Boolean)
  return segments.length === 1
}

/**
 * Filter items to only immediate children of a parent path
 */
export function getImmediateChildren(items: ItemData[], parentPath: string): ItemData[] {
  return items.filter(item => isImmediateChild(item.item_path, parentPath))
}

/**
 * Sort items: directories first, then files, alphabetically within each group
 */
export function sortTreeItems(items: TreeNodeData[]): TreeNodeData[] {
  return [...items].sort((a, b) => {
    // Directories before files
    if (a.item_type === 'D' && b.item_type !== 'D') return -1
    if (a.item_type !== 'D' && b.item_type === 'D') return 1

    // Alphabetical within group (case-insensitive)
    return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
  })
}

/**
 * Transform ItemData to TreeNodeData
 */
export function itemToTreeNode(item: ItemData): TreeNodeData {
  return {
    ...item,
    name: item.item_name, // Use backend-parsed name instead of client-side parsing
    children: undefined,
    isExpanded: false,
    isLoading: false,
  }
}
