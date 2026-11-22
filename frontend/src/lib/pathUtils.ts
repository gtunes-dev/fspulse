// Path utility functions for file tree navigation

/**
 * Shorten a filesystem path for display by collapsing middle segments.
 * Example: "/Users/alice/Documents/projects/myapp" → "/Users/.../myapp"
 */
export function shortenPath(path: string, maxLength: number = 30): string {
  if (path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return `${parts[0]}/.../${parts[parts.length - 1]}`
}

export type ChangeKind = 'added' | 'modified' | 'deleted' | 'unchanged'
export type HashState = 'unknown' | 'baseline' | 'suspect'
export type ValState = 'unknown' | 'valid' | 'invalid' | 'no_validator'

/** How the time API describes an item's change over a range. */
export type TimelineChange =
  | 'added' | 'modified' | 'deleted' | 'unchanged' | 'transient' | 'ignored' | 'resumed'

/**
 * What Browse shows for an item the time API returned.
 *
 * Ignored and resumed are not changes to the item on disk, so they are
 * not change kinds: the item keeps the neutral kind and carries its own
 * note (`ignored_since`, `resumed`), which is shown in words.
 */
export function describeTimelineItem(item: {
  is_ignored: boolean
  first_seen_at: number
  change: TimelineChange | null
}): { change_kind: ChangeKind; ignored_since: number | null; resumed: boolean } {
  const ignored = item.is_ignored
  const change = item.change
  const plain =
    change === 'added' || change === 'modified' || change === 'deleted' ? change : 'unchanged'
  return {
    // A point in time has no comparison: everything is as it was.
    change_kind: ignored ? 'unchanged' : plain,
    ignored_since: ignored ? item.first_seen_at : null,
    resumed: !ignored && change === 'resumed',
  }
}

/** What was done to an item inside the selected range, each at least once. */
export interface Happened {
  added: boolean
  modified: boolean
  deleted: boolean
}

/**
 * The four numbers behind a row's change square: how many items, the
 * row's own item and (for a folder) everything beneath it, were added,
 * modified and deleted at least once inside the range. One item can be
 * in more than one of them.
 *
 * `unchanged_count` is not a count. For a file it is 1 when nothing
 * happened to it. For a folder it is 1 when the folder exists at the
 * end of the range and so may hold unchanged items, which keeps it
 * visible and expandable when "Unchanged" is shown; an ignored folder
 * stays expandable too, so what was recorded beneath it can be shown.
 */
export function changeCounts(item: {
  item_type: string
  is_deleted: boolean
  is_ignored: boolean
  /** Null for a point in time. */
  happened?: Happened | null
  /** Folders in a range: what happened beneath the folder. */
  descendants?: { added: number; modified: number; deleted: number } | null
}): { add_count: number; modify_count: number; delete_count: number; unchanged_count: number } {
  const isDir = item.item_type === 'D'
  const beneath = isDir ? item.descendants : null
  const add_count = (item.happened?.added ? 1 : 0) + (beneath?.added ?? 0)
  const modify_count = (item.happened?.modified ? 1 : 0) + (beneath?.modified ?? 0)
  const delete_count = (item.happened?.deleted ? 1 : 0) + (beneath?.deleted ?? 0)
  const unchanged_count = isDir
    ? (item.is_deleted && !item.is_ignored ? 0 : 1)
    : (add_count + modify_count + delete_count === 0 ? 1 : 0)
  return { add_count, modify_count, delete_count, unchanged_count }
}

export function hashStateFromInt(val: number | null): HashState | null {
  if (val === null || val === undefined) return null
  switch (val) {
    case 0: return 'unknown'
    case 1: return 'baseline'
    case 2: return 'suspect'
    default: return 'unknown'
  }
}

export function valStateFromInt(val: number | null): ValState | null {
  if (val === null || val === undefined) return null
  switch (val) {
    case 0: return 'unknown'
    case 1: return 'valid'
    case 2: return 'invalid'
    case 3: return 'no_validator'
    default: return 'unknown'
  }
}

/**
 * Whether an item is shown, given the change kinds that are hidden: it
 * is if any kind lit in its change square is not hidden. See
 * `changeCounts` for what lights each kind.
 */
export function isItemVisible(
  item: {
    add_count?: number | null
    modify_count?: number | null
    delete_count?: number | null
    unchanged_count?: number | null
    ignored_since?: number | null
    resumed?: boolean
  },
  hiddenKinds: Set<ChangeKind>,
): boolean {
  // Ignored and resumed items are listed because of that, whatever the
  // change-type filter says.
  if (item.ignored_since != null || item.resumed) return true
  return (!hiddenKinds.has('added') && (item.add_count ?? 0) > 0) ||
    (!hiddenKinds.has('modified') && (item.modify_count ?? 0) > 0) ||
    (!hiddenKinds.has('deleted') && (item.delete_count ?? 0) > 0) ||
    (!hiddenKinds.has('unchanged') && (item.unchanged_count ?? 0) > 0)
}

export interface ItemData {
  item_id: number
  item_path: string
  item_name: string // Filename/directory name (from backend using @name format)
  item_type: 'F' | 'D' | 'S' | 'O' // File, Directory, Symlink, Other
  is_deleted: boolean
  /** When tracking stopped because of an ignore pattern; null if tracked. */
  ignored_since?: number | null
  /** Ignored at the start of the range, tracked again at the end. */
  resumed?: boolean
  size?: number | null
  mod_date?: number | null
  change_kind: ChangeKind
  // What happened to the item, and beneath it if it is a folder, inside
  // the range. See `changeCounts`.
  add_count?: number | null
  modify_count?: number | null
  delete_count?: number | null
  unchanged_count?: number | null
  // Integrity state for files (null for dirs/symlinks/other)
  hash_state?: HashState | null
  val_state?: ValState | null
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
  ignored_since?: number | null
  resumed?: boolean
  size?: number | null
  mod_date?: number | null
  change_kind: ChangeKind
  add_count?: number | null
  modify_count?: number | null
  delete_count?: number | null
  unchanged_count?: number | null
  hash_state?: HashState | null
  val_state?: ValState | null
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
