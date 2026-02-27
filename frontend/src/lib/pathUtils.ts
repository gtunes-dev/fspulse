// Path utility functions for file tree navigation

export type ChangeKind = 'added' | 'modified' | 'deleted' | 'unchanged'

/**
 * Determines if an item should be visible given the set of hidden change kinds.
 *
 * For files: visible if their change_kind is not hidden.
 * For folders: visible if their own change_kind is not hidden, OR if they have
 * descendant changes of any visible kind (based on add_count/modify_count/delete_count).
 */
export function isItemVisible(
  item: { item_type: string; change_kind: ChangeKind; add_count?: number | null; modify_count?: number | null; delete_count?: number | null; unchanged_count?: number | null },
  hiddenKinds: Set<ChangeKind>,
): boolean {
  // Own change_kind is visible — always show
  if (!hiddenKinds.has(item.change_kind)) return true

  // For non-directories, that's the only check
  if (item.item_type !== 'D') return false

  // Directory with hidden own kind — check if it has visible descendant changes.
  // Counts are always accurate: changed folders have per-scan counts from the
  // backend; unchanged folders have derived counts (0 adds/mods/dels, unchanged
  // = total alive from the temporal version) computed at the data mapping layer.
  if (!hiddenKinds.has('added') && (item.add_count ?? 0) > 0) return true
  if (!hiddenKinds.has('modified') && (item.modify_count ?? 0) > 0) return true
  if (!hiddenKinds.has('deleted') && (item.delete_count ?? 0) > 0) return true
  if (!hiddenKinds.has('unchanged') && (item.unchanged_count ?? 0) > 0) return true

  return false
}

export interface ItemData {
  item_id: number
  item_path: string
  item_name: string // Filename/directory name (from backend using @name format)
  item_type: 'F' | 'D' | 'S' | 'O' // File, Directory, Symlink, Other
  is_deleted: boolean
  size?: number | null
  mod_date?: number | null
  change_kind: ChangeKind
  add_count?: number | null       // Folder descendant add count (null for files)
  modify_count?: number | null    // Folder descendant modify count (null for files)
  delete_count?: number | null    // Folder descendant delete count (null for files)
  unchanged_count?: number | null // Folder descendant unchanged count (null for files)
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
  size?: number | null
  mod_date?: number | null
  change_kind: ChangeKind
  add_count?: number | null
  modify_count?: number | null
  delete_count?: number | null
  unchanged_count?: number | null
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
 * Get the chain of ancestor directory paths that must be expanded
 * to reveal targetPath in a tree rooted at rootPath.
 *
 * Returns paths from root's first child down to target's PARENT.
 * Does not include rootPath (root items are already visible) or
 * targetPath itself (we want to reveal it, not expand it).
 *
 * Example: getAncestorChain("/home", "/home/a/b/c") → ["/home/a", "/home/a/b"]
 * Example: getAncestorChain("/home", "/home/a") → [] (a is a root item, already visible)
 * Example: getAncestorChain("/home", "/home") → []
 */
export function getAncestorChain(rootPath: string, targetPath: string): string[] {
  const normalizedRoot = rootPath.replace(/\/$/, '')
  const normalizedTarget = targetPath.replace(/\/$/, '')
  if (normalizedTarget === normalizedRoot) return []

  const relative = normalizedTarget.substring(normalizedRoot.length)
  const segments = relative.split('/').filter(Boolean)

  // Remove the target itself — we only need ancestors
  segments.pop()

  const chain: string[] = []
  let current = normalizedRoot
  for (const seg of segments) {
    current = current + '/' + seg
    chain.push(current)
  }
  return chain
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
