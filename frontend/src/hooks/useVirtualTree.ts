import { useState, useCallback, useRef } from 'react'
import { sortTreeItems, getAncestorChain, type TreeNodeData, type FlatTreeItem } from '@/lib/pathUtils'
import type { CachedItem } from './useBrowseCache'

export type { FlatTreeItem }

interface UseVirtualTreeOptions {
  loadChildrenFn: (parentPath: string) => Promise<CachedItem[]>
}

/**
 * Hook for managing a virtualized tree structure with lazy loading.
 *
 * This hook maintains a flat array of tree items and handles expansion/collapse
 * logic with on-demand loading of children via the provided loadChildrenFn
 * (typically backed by useBrowseCache).
 *
 * @param options - Configuration including loadChildrenFn for fetching children
 * @returns Tree state and operations (flatItems, initializeTree, toggleNode, isLoading, revealPath)
 */
export function useVirtualTree(options: UseVirtualTreeOptions) {
  const { loadChildrenFn } = options
  const [flatItems, setFlatItems] = useState<FlatTreeItem[]>([])
  const [loadingItems, setLoadingItems] = useState<Set<number>>(new Set())

  // Refs for synchronous access to avoid stale closure issues
  const flatItemsRef = useRef<FlatTreeItem[]>([])
  const loadingItemsRef = useRef<Set<number>>(new Set())

  // Generation counter — incremented on tree init/rebuild.
  // Used by loadChildren to discard stale results from clicks during a rebuild.
  const generationRef = useRef(0)

  // Keep ref in sync with state (also updated inside setFlatItems for revealPath)
  flatItemsRef.current = flatItems

  /**
   * Initializes the tree with root-level items.
   * This should be called once after fetching the top-level directory contents.
   */
  const initializeTree = useCallback((rootItems: TreeNodeData[]) => {
    generationRef.current++
    const initialFlatItems: FlatTreeItem[] = rootItems.map(item => ({
      item_id: item.item_id,
      item_path: item.item_path,
      item_name: item.item_name,
      item_type: item.item_type,
      is_deleted: item.is_deleted,
      size: item.size,
      mod_date: item.mod_date,
      change_kind: item.change_kind,
      add_count: item.add_count,
      modify_count: item.modify_count,
      delete_count: item.delete_count,
      unchanged_count: item.unchanged_count,
      hash_state: item.hash_state,
      val_state: item.val_state,
      depth: 0,
      isExpanded: false,
      childrenLoaded: false,
      hasChildren: item.item_type === 'D',
    }))
    flatItemsRef.current = initialFlatItems
    setFlatItems(initialFlatItems)
  }, [])

  /**
   * Collapses a directory node and removes all its descendants from the flat array.
   * Also resets childrenLoaded flag so re-expansion will fetch again (cache makes this instant).
   */
  const collapseNode = useCallback((itemId: number) => {
    setFlatItems(prev => {
      const itemIndex = prev.findIndex(item => item.item_id === itemId)
      if (itemIndex === -1) return prev

      const item = prev[itemIndex]

      // Find all descendants (items that come after this one with greater depth)
      const descendants: number[] = []
      for (let i = itemIndex + 1; i < prev.length; i++) {
        if (prev[i].depth <= item.depth) {
          break // No longer in this subtree
        }
        descendants.push(i)
      }

      // Remove descendants and mark as collapsed
      const updated = [...prev]
      updated[itemIndex] = {
        ...updated[itemIndex],
        isExpanded: false,
        childrenLoaded: false // Reset so re-expand will load again (instant from cache)
      }

      // Remove descendants in reverse order to maintain indices
      for (let i = descendants.length - 1; i >= 0; i--) {
        updated.splice(descendants[i], 1)
      }

      flatItemsRef.current = updated
      return updated
    })
  }, [])

  /**
   * Expands a directory node. Assumes children are already in the flat array.
   * Use loadChildren() first if children haven't been loaded yet.
   */
  const expandNode = useCallback((itemId: number) => {
    setFlatItems(prev => {
      const itemIndex = prev.findIndex(item => item.item_id === itemId)
      if (itemIndex === -1) return prev

      // Mark as expanded
      const updated = [...prev]
      updated[itemIndex] = { ...updated[itemIndex], isExpanded: true }
      flatItemsRef.current = updated
      return updated
    })
  }, [])

  /**
   * Loads children for a directory node from the shared cache.
   * Children are always loaded with deleted items - filtering happens client-side.
   */
  const loadChildren = useCallback(async (itemId: number, parentPath: string, parentDepth: number) => {
    // Check ref synchronously to prevent race conditions with stale closures
    if (loadingItemsRef.current.has(itemId)) {
      return
    }

    // Add to both ref (immediate) and state (for UI)
    loadingItemsRef.current.add(itemId)
    setLoadingItems(prev => new Set(prev).add(itemId))

    const gen = generationRef.current

    try {
      const items = await loadChildrenFn(parentPath)

      // Discard results if the tree was reinitialized while we were loading
      if (generationRef.current !== gen) return

      // Transform to TreeNodeData format for sorting
      const childItems: TreeNodeData[] = items.map(item => ({
        item_id: item.item_id,
        item_path: item.item_path,
        item_name: item.item_name,
        item_type: item.item_type,
        is_deleted: item.is_deleted,
        size: item.size,
        mod_date: item.mod_date,
        change_kind: item.change_kind,
        add_count: item.add_count,
        modify_count: item.modify_count,
        delete_count: item.delete_count,
        unchanged_count: item.unchanged_count,
        hash_state: item.hash_state,
        val_state: item.val_state,
        name: item.item_name,
      }))

      // Sort children (directories first, then alphabetically)
      const sortedChildren = sortTreeItems(childItems)

      // Insert children into flat array
      setFlatItems(prev => {
        const itemIndex = prev.findIndex(item => item.item_id === itemId)
        if (itemIndex === -1) return prev

        const updated = [...prev]

        // Mark parent as expanded and children loaded
        updated[itemIndex] = {
          ...updated[itemIndex],
          isExpanded: true,
          childrenLoaded: true,
        }

        // Convert children to FlatTreeItems
        const childFlatItems: FlatTreeItem[] = sortedChildren.map(child => ({
          item_id: child.item_id,
          item_path: child.item_path,
          item_name: child.item_name,
          item_type: child.item_type,
          is_deleted: child.is_deleted,
          size: child.size,
          mod_date: child.mod_date,
          change_kind: child.change_kind,
          add_count: child.add_count,
          modify_count: child.modify_count,
          delete_count: child.delete_count,
          unchanged_count: child.unchanged_count,
          hash_state: child.hash_state,
          val_state: child.val_state,
          depth: parentDepth + 1,
          isExpanded: false,
          childrenLoaded: false,
          hasChildren: child.item_type === 'D',
        }))

        // Insert children after parent
        updated.splice(itemIndex + 1, 0, ...childFlatItems)

        // Sync ref immediately so revealPath can read updated state
        flatItemsRef.current = updated
        return updated
      })
    } catch (error) {
      console.error('Error loading children:', error)
    } finally {
      // Remove from both ref and state
      loadingItemsRef.current.delete(itemId)
      setLoadingItems(prev => {
        const updated = new Set(prev)
        updated.delete(itemId)
        return updated
      })
    }
  }, [loadChildrenFn])

  /**
   * Toggles the expansion state of a directory node.
   * If collapsed, expands it (loading children if needed).
   * If expanded, collapses it and removes descendants from view.
   */
  const toggleNode = useCallback(async (itemId: number) => {
    // Use ref to get current state and avoid stale closure issues
    const item = flatItemsRef.current.find(i => i.item_id === itemId)
    if (!item || !item.hasChildren) return

    // If currently expanded, collapse it
    if (item.isExpanded) {
      collapseNode(itemId)
      return
    }

    // If not expanded, expand it
    // If children not loaded, fetch them first
    if (!item.childrenLoaded) {
      await loadChildren(itemId, item.item_path, item.depth)
    } else {
      // Children already loaded, just expand
      expandNode(itemId)
    }
  }, [collapseNode, loadChildren, expandNode])

  /**
   * Reveals a target path in the tree by expanding all ancestor directories.
   * Returns the item_id of the target if found, or null if the path doesn't exist.
   *
   * This is used for Folder → Tree sync: when switching from Folder view to Tree view,
   * we expand the ancestors so the user's current folder location is visible.
   */
  const revealPath = useCallback(async (targetPath: string, rootPath: string): Promise<number | null> => {
    const chain = getAncestorChain(rootPath, targetPath)

    for (const ancestorPath of chain) {
      const item = flatItemsRef.current.find(i => i.item_path === ancestorPath)
      if (!item) return null // Path not found in tree

      // Only expand directories that aren't already expanded
      if (item.hasChildren && !item.isExpanded) {
        await loadChildren(item.item_id, item.item_path, item.depth)
      }
    }

    // Find the target item
    const target = flatItemsRef.current.find(i => i.item_path === targetPath)
    return target?.item_id ?? null
  }, [loadChildren])

  /**
   * Returns the set of currently expanded directory paths.
   * Reads from ref (not state) so it can be called from effects without dep issues.
   */
  const getExpandedPaths = useCallback((): Set<string> => {
    return new Set(
      flatItemsRef.current.filter(i => i.isExpanded).map(i => i.item_path)
    )
  }, [])

  /**
   * Initializes the tree with root-level items, restoring previous expansion state.
   * Builds the complete flat array in memory (parallel fetches per level) before
   * setting state — resulting in a single render with no flicker.
   */
  const initializeTreeWithExpansions = useCallback(async (
    rootItems: TreeNodeData[],
    expandedPaths: Set<string>,
  ) => {
    generationRef.current++
    const result: FlatTreeItem[] = []

    // Helper to convert a TreeNodeData to FlatTreeItem
    const toFlatItem = (item: TreeNodeData, depth: number, isExpanded: boolean): FlatTreeItem => ({
      item_id: item.item_id,
      item_path: item.item_path,
      item_name: item.item_name,
      item_type: item.item_type,
      is_deleted: item.is_deleted,
      size: item.size,
      mod_date: item.mod_date,
      change_kind: item.change_kind,
      add_count: item.add_count,
      modify_count: item.modify_count,
      delete_count: item.delete_count,
      unchanged_count: item.unchanged_count,
      hash_state: item.hash_state,
      val_state: item.val_state,
      depth,
      isExpanded,
      childrenLoaded: isExpanded,
      hasChildren: item.item_type === 'D',
    })

    // Helper to convert CachedItem to TreeNodeData
    const toTreeNode = (item: CachedItem): TreeNodeData => ({
      item_id: item.item_id,
      item_path: item.item_path,
      item_name: item.item_name,
      item_type: item.item_type,
      is_deleted: item.is_deleted,
      size: item.size,
      mod_date: item.mod_date,
      change_kind: item.change_kind,
      add_count: item.add_count,
      modify_count: item.modify_count,
      delete_count: item.delete_count,
      unchanged_count: item.unchanged_count,
      hash_state: item.hash_state,
      val_state: item.val_state,
      name: item.item_name,
    })

    async function buildLevel(items: TreeNodeData[], depth: number) {
      // Identify directories that were previously expanded
      const toExpand = items.filter(
        item => item.item_type === 'D' && expandedPaths.has(item.item_path)
      )

      // Fetch all children at this level in parallel
      const childrenMap = new Map<string, TreeNodeData[]>()
      await Promise.all(
        toExpand.map(async item => {
          try {
            const children = await loadChildrenFn(item.item_path)
            childrenMap.set(item.item_path, sortTreeItems(children.map(toTreeNode)))
          } catch {
            // Path doesn't exist in new scan — branch stays collapsed
          }
        })
      )

      // Build flat array in depth-first order
      for (const item of items) {
        const children = childrenMap.get(item.item_path)
        const isExpanded = children != null
        result.push(toFlatItem(item, depth, isExpanded))

        if (children) {
          await buildLevel(children, depth + 1)
        }
      }
    }

    await buildLevel(rootItems, 0)

    // Single state update — one render
    flatItemsRef.current = result
    setFlatItems(result)
  }, [loadChildrenFn])

  /**
   * Checks if a specific node is currently loading its children.
   */
  const isLoading = useCallback((itemId: number) => loadingItems.has(itemId), [loadingItems])

  return {
    flatItems,
    initializeTree,
    initializeTreeWithExpansions,
    getExpandedPaths,
    toggleNode,
    isLoading,
    revealPath,
  }
}
