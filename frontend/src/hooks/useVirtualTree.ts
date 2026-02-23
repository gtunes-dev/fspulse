import { useState, useCallback, useRef } from 'react'
import { sortTreeItems, type TreeNodeData, type FlatTreeItem } from '@/lib/pathUtils'

export type { FlatTreeItem }

interface UseVirtualTreeOptions {
  rootId: number
  scanId: number
}

/**
 * Response type from the temporal /api/items/immediate-children endpoint
 */
interface ImmediateChildrenResponse {
  item_id: number
  item_path: string
  item_name: string
  item_type: string
  is_deleted: boolean
  size: number | null
  mod_date: number | null
}

/**
 * Hook for managing a virtualized tree structure with lazy loading
 * using the temporal item_versions model.
 *
 * This hook maintains a flat array of tree items and handles expansion/collapse
 * logic with on-demand loading of children from the backend API.
 *
 * @param options - Configuration including rootId and scanId for API calls
 * @returns Tree state and operations (flatItems, initializeTree, toggleNode, isLoading)
 */
export function useVirtualTree(options: UseVirtualTreeOptions) {
  const { rootId, scanId } = options
  const [flatItems, setFlatItems] = useState<FlatTreeItem[]>([])
  const [loadingItems, setLoadingItems] = useState<Set<number>>(new Set())

  // Refs for synchronous access to avoid stale closure issues
  const flatItemsRef = useRef<FlatTreeItem[]>([])
  const loadingItemsRef = useRef<Set<number>>(new Set())

  // Keep ref in sync with state
  flatItemsRef.current = flatItems

  /**
   * Initializes the tree with root-level items.
   * This should be called once after fetching the top-level directory contents.
   */
  const initializeTree = useCallback((rootItems: TreeNodeData[]) => {
    const initialFlatItems: FlatTreeItem[] = rootItems.map(item => ({
      item_id: item.item_id,
      item_path: item.item_path,
      item_name: item.item_name,
      item_type: item.item_type,
      is_deleted: item.is_deleted,
      size: item.size,
      mod_date: item.mod_date,
      depth: 0,
      isExpanded: false,
      childrenLoaded: false,
      hasChildren: item.item_type === 'D',
    }))
    setFlatItems(initialFlatItems)
  }, [])

  /**
   * Collapses a directory node and removes all its descendants from the flat array.
   * Also resets childrenLoaded flag so re-expansion will fetch fresh data.
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
        childrenLoaded: false // Reset so re-expand will fetch again
      }

      // Remove descendants in reverse order to maintain indices
      for (let i = descendants.length - 1; i >= 0; i--) {
        updated.splice(descendants[i], 1)
      }

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
      return updated
    })
  }, [])

  /**
   * Loads children for a directory node from the temporal API.
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

    try {
      const params = new URLSearchParams({
        root_id: rootId.toString(),
        parent_path: parentPath,
        scan_id: scanId.toString(),
      })

      const url = `/api/items/immediate-children?${params}`
      const response = await fetch(url)
      if (!response.ok) {
        throw new Error(`Failed to fetch children: ${response.statusText}`)
      }

      const items = await response.json() as ImmediateChildrenResponse[]

      // Transform API response to TreeNodeData format
      const childItems: TreeNodeData[] = items.map(item => ({
        item_id: item.item_id,
        item_path: item.item_path,
        item_name: item.item_name,
        item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
        is_deleted: item.is_deleted,
        size: item.size,
        mod_date: item.mod_date,
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
          depth: parentDepth + 1,
          isExpanded: false,
          childrenLoaded: false,
          hasChildren: child.item_type === 'D',
        }))

        // Insert children after parent
        updated.splice(itemIndex + 1, 0, ...childFlatItems)

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
  }, [rootId, scanId])

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
   * Checks if a specific node is currently loading its children.
   */
  const isLoading = useCallback((itemId: number) => loadingItems.has(itemId), [loadingItems])

  return {
    flatItems,
    initializeTree,
    toggleNode,
    isLoading,
  }
}
