import { useState, useEffect, useCallback, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'
import { FolderTree, FolderOpen, Search, ArrowLeftRight, SlidersHorizontal, Plus, Triangle, X, Minus } from 'lucide-react'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { cn } from '@/lib/utils'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { RootPicker } from '@/components/shared/RootPicker'
import { CompactScanBar } from '@/components/shared/CompactScanBar'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { ItemDetail } from '@/components/shared/ItemDetail'
import { useBrowseCache } from '@/hooks/useBrowseCache'
import { getParentPath, type ChangeKind } from '@/lib/pathUtils'
import { FileTreeView } from './FileTreeView'
import type { FileTreeViewHandle } from './FileTreeView'
import { FolderView } from './FolderView'
import { SearchResultsList } from './SearchResultsList'

interface Root {
  root_id: number
  root_path: string
}

type ViewMode = 'tree' | 'folder' | 'search'

interface SelectedItem {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
}

interface BrowseCardProps {
  roots: Root[]
  defaultRootId?: string
  defaultScanId?: number
  isActive?: boolean
  isPrimary?: boolean
}

export function BrowseCard({ roots, defaultRootId, defaultScanId, isActive: pageActive = true, isPrimary = false }: BrowseCardProps) {
  const [, setSearchParams] = useSearchParams()
  const [selectedRootId, setSelectedRootIdRaw] = useState<string>(defaultRootId ?? '')

  // Wrap setter to also update URL for the primary card (enables sidebar root context)
  const setSelectedRootId = useCallback((rootId: string) => {
    setSelectedRootIdRaw(rootId)
    if (isPrimary && rootId) {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev)
        next.set('root_id', rootId)
        return next
      }, { replace: true })
    }
  }, [isPrimary, setSearchParams])
  const [resolvedScanId, setResolvedScanId] = useState<number | null>(null)
  const [scanStatus, setScanStatus] = useState<'resolving' | 'resolved' | 'no-scan'>('resolving')
  const [viewMode, setViewMode] = useState<ViewMode>('tree')
  const [hiddenKinds, setHiddenKinds] = useState<Set<ChangeKind>>(new Set())
  const [searchFilter, setSearchFilter] = useState('')
  const [debouncedSearch, setDebouncedSearch] = useState('')
  // Per-view selection: each tab has its own independently selected item
  const [selectedItems, setSelectedItems] = useState<Record<ViewMode, SelectedItem | null>>({
    tree: null,
    folder: null,
    search: null,
  })

  // Detail panel placement (right card can flip)
  const [detailOnRight, setDetailOnRight] = useState(true)

  // Lifted folder navigation path
  const [folderCurrentPath, setFolderCurrentPath] = useState<string>('')

  // Refs for view sync
  const treeRef = useRef<FileTreeViewHandle>(null)
  const previousViewRef = useRef<ViewMode>('tree')

  // Shared data cache for Tree and Folder views
  const selectedRoot = roots.find(r => r.root_id.toString() === selectedRootId)
  const cache = useBrowseCache(selectedRoot?.root_id ?? 0, resolvedScanId ?? 0)

  // Sync with URL param changes (e.g., deep-linking from Dashboard)
  useEffect(() => {
    if (defaultRootId && defaultRootId !== selectedRootId) {
      setSelectedRootId(defaultRootId)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [defaultRootId])

  // Auto-select first root if no default provided
  useEffect(() => {
    if (!selectedRootId && roots.length > 0) {
      setSelectedRootId(roots[0].root_id.toString())
    }
  }, [roots, selectedRootId, setSelectedRootId])

  // Reset resolved scan, all selections, and folder path when root changes
  useEffect(() => {
    setResolvedScanId(null)
    setScanStatus('resolving')
    setSelectedItems({ tree: null, folder: null, search: null })
  }, [selectedRootId])

  // Reset folder path when root changes
  useEffect(() => {
    if (selectedRoot) {
      setFolderCurrentPath(selectedRoot.root_path)
    }
  }, [selectedRoot])

  const handleScanResolved = useCallback((scanId: number) => {
    setResolvedScanId(scanId)
    setScanStatus('resolved')
  }, [])

  const handleNoScan = useCallback(() => {
    setResolvedScanId(null)
    setScanStatus('no-scan')
  }, [])

  // Debounce search input
  useEffect(() => {
    const timeout = setTimeout(() => {
      setDebouncedSearch(searchFilter)
    }, 300)
    return () => clearTimeout(timeout)
  }, [searchFilter])

  // Auto-switch to search mode when user types
  const handleSearchChange = (value: string) => {
    setSearchFilter(value)
    if (value.trim().length > 0 && viewMode !== 'search') {
      setViewMode('search')
    }
  }

  // ── View sync logic ──────────────────────────────────────────────────

  const syncFolderToTree = useCallback(() => {
    if (!selectedRoot) return

    // Reveal the folder's current path in the tree (async, uses rAF to let
    // the tree's isActive effect fire first on initial visit)
    if (treeRef.current) {
      requestAnimationFrame(() => {
        treeRef.current?.revealPath(folderCurrentPath)
      })
    }
  }, [folderCurrentPath, selectedRoot])

  const handleViewModeChange = useCallback((value: string) => {
    const newMode = value as ViewMode
    const prevMode = previousViewRef.current
    previousViewRef.current = newMode

    // Copy selection synchronously BEFORE changing viewMode so the detail
    // panel never unmounts/remounts when the same item stays selected.
    if (prevMode === 'tree' && newMode === 'folder') {
      const treeSelection = selectedItems.tree
      if (treeSelection && selectedRoot) {
        const targetPath = treeSelection.itemType === 'D'
          ? treeSelection.itemPath
          : getParentPath(treeSelection.itemPath)
        setFolderCurrentPath(targetPath)
        setSelectedItems(prev => ({ ...prev, folder: treeSelection }))
      }
    } else if (prevMode === 'folder' && newMode === 'tree') {
      const folderSelection = selectedItems.folder
      if (folderSelection) {
        setSelectedItems(prev => ({ ...prev, tree: folderSelection }))
      }
      syncFolderToTree()
    }

    setViewMode(newMode)
  }, [selectedItems.tree, selectedItems.folder, selectedRoot, syncFolderToTree])

  // ── Selection handlers ───────────────────────────────────────────────

  const toSelectedItem = (item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }): SelectedItem => ({
    itemId: item.itemId,
    itemPath: item.itemPath,
    itemType: item.itemType as 'F' | 'D' | 'S' | 'O',
    isTombstone: item.isTombstone,
  })

  const handleTreeSelect = useCallback((item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => {
    setSelectedItems(prev => ({ ...prev, tree: toSelectedItem(item) }))
  }, [])

  const handleFolderSelect = useCallback((item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => {
    setSelectedItems(prev => ({ ...prev, folder: toSelectedItem(item) }))
  }, [])

  const handleSearchSelect = useCallback((item: { itemId: number; itemPath: string; itemType: string; isTombstone: boolean }) => {
    setSelectedItems(prev => ({ ...prev, search: toSelectedItem(item) }))
  }, [])

  const handleDetailClose = useCallback(() => {
    setSelectedItems(prev => ({ ...prev, [viewMode]: null }))
  }, [viewMode])

  const hasSearchQuery = debouncedSearch.trim().length > 0
  const activeSelection = selectedItems[viewMode]

  // Build the content view — all views always mounted, toggled via CSS
  const contentView = selectedRoot && resolvedScanId ? (
    <div className="flex-1 min-w-0 flex flex-col">
      {/* Tree View — always rendered */}
      <div className={viewMode === 'tree' ? '' : 'hidden'}>
        <FileTreeView
          ref={treeRef}
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          cache={cache}
          hiddenKinds={hiddenKinds}
          isActive={pageActive && viewMode === 'tree'}
          selectedItemId={selectedItems.tree?.itemId}
          onItemSelect={handleTreeSelect}
        />
      </div>

      {/* Folder View — always rendered */}
      <div className={viewMode === 'folder' ? '' : 'hidden'}>
        <FolderView
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          cache={cache}
          currentPath={folderCurrentPath}
          onNavigate={setFolderCurrentPath}
          hiddenKinds={hiddenKinds}
          isActive={pageActive && viewMode === 'folder'}
          selectedItemId={selectedItems.folder?.itemId}
          onItemSelect={handleFolderSelect}
        />
      </div>

      {/* Search Results — always rendered */}
      <div className={viewMode === 'search' && hasSearchQuery ? '' : 'hidden'}>
        <SearchResultsList
          rootId={selectedRoot.root_id}
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          searchQuery={debouncedSearch}
          hiddenKinds={hiddenKinds}
          isActive={pageActive && viewMode === 'search' && hasSearchQuery}
          selectedItemId={selectedItems.search?.itemId}
          onItemSelect={handleSearchSelect}
        />
      </div>

      {/* Search placeholder when no query */}
      {viewMode === 'search' && !hasSearchQuery && (
        <div className="flex-1 min-h-0">
          <div className="flex items-center justify-center h-32 text-muted-foreground">
            No results found
          </div>
        </div>
      )}
    </div>
  ) : (
    <div className="flex-1 min-h-0 min-w-0">
      <div className="flex items-center justify-center h-full text-muted-foreground">
        {!selectedRoot
          ? 'Select a root'
          : scanStatus === 'no-scan'
            ? 'No scan data available'
            : 'Resolving scan...'}
      </div>
    </div>
  )

  // Detail panel shows the active view's selection
  const detailPanel = activeSelection && selectedRoot && resolvedScanId ? (
    <div className={cn(
      "w-[500px] flex-shrink-0",
      detailOnRight ? 'border-l border-border' : 'border-r border-border'
    )}>
      <div className="sticky top-0">
        <ItemDetail
          mode="panel"
          itemId={activeSelection.itemId}
          itemPath={activeSelection.itemPath}
          itemType={activeSelection.itemType}
          isTombstone={activeSelection.isTombstone}
          scanId={resolvedScanId}
          onClose={handleDetailClose}
        />
      </div>
    </div>
  ) : null

  return (
    <Card className="flex-1 min-w-0 flex flex-col">
      <CardHeader>
        <RootPicker
          roots={roots}
          value={selectedRootId}
          onChange={setSelectedRootId}
          variant="title"
        />
      </CardHeader>
      <CardContent className="flex-1 flex flex-col gap-3">
        {/* Compact scan bar */}
        {selectedRoot && (
          <CompactScanBar
            rootId={selectedRoot.root_id}
            initialScanId={defaultScanId}
            onScanResolved={handleScanResolved}
            onNoScan={handleNoScan}
          />
        )}

        {/* View mode tabs + search + layout control */}
        <div className="flex items-center gap-3">
          <Tabs value={viewMode} onValueChange={handleViewModeChange}>
            <TabsList className="shrink-0">
              <TabsTrigger value="tree" className="gap-1.5">
                <FolderTree className="h-3.5 w-3.5" />
                Tree
              </TabsTrigger>
              <TabsTrigger value="folder" className="gap-1.5">
                <FolderOpen className="h-3.5 w-3.5" />
                Folder
              </TabsTrigger>
              <TabsTrigger value="search" className="gap-1.5">
                <Search className="h-3.5 w-3.5" />
                Search
              </TabsTrigger>
            </TabsList>
          </Tabs>

          {viewMode === 'search' && (
            <SearchFilter
              value={searchFilter}
              onChange={handleSearchChange}
              placeholder="Search files and folders..."
            />
          )}

          <div className="flex-1" />

          <button
            className="p-1 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground"
            onClick={() => setDetailOnRight(prev => !prev)}
            aria-label="Flip detail panel side"
          >
            <ArrowLeftRight className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Filter bar */}
        <div className="flex items-center gap-3 px-3 py-1.5 border border-border rounded-lg bg-muted/30">
          <SlidersHorizontal className="h-3.5 w-3.5 text-muted-foreground flex-shrink-0" />
          <span className="text-xs font-medium text-muted-foreground flex-shrink-0">Change Type</span>
          <div className="flex items-center gap-1">
            {([
              { kind: 'added' as ChangeKind, label: 'Added', color: 'text-green-500', Icon: Plus, filled: false },
              { kind: 'modified' as ChangeKind, label: 'Modified', color: 'text-blue-500', Icon: Triangle, filled: true },
              { kind: 'deleted' as ChangeKind, label: 'Deleted', color: 'text-red-500', Icon: X, filled: false },
              { kind: 'unchanged' as ChangeKind, label: 'Unchanged', color: 'text-foreground', Icon: Minus, filled: false },
            ]).map(({ kind, label, color, Icon, filled }) => {
              const visible = !hiddenKinds.has(kind)
              return (
                <button
                  key={kind}
                  className={cn(
                    'inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-xs cursor-pointer transition-colors whitespace-nowrap',
                    visible
                      ? 'text-foreground hover:bg-accent'
                      : 'text-muted-foreground/40 hover:bg-accent/50'
                  )}
                  onClick={() => setHiddenKinds(prev => {
                    const next = new Set(prev)
                    if (next.has(kind)) next.delete(kind)
                    else next.add(kind)
                    return next
                  })}
                >
                  <Icon
                    className={cn(
                      'h-4 w-4 flex-shrink-0 transition-all',
                      visible ? color : 'text-muted-foreground/25'
                    )}
                    {...(filled ? { fill: 'currentColor' } : {})}
                  />
                  {label}
                </button>
              )
            })}
          </div>
        </div>

        {/* Content area: content view + detail panel side by side */}
        <div className="flex-1 flex border border-border rounded-lg overflow-hidden">
          {!detailOnRight && detailPanel}
          {contentView}
          {detailOnRight && detailPanel}
        </div>
      </CardContent>
    </Card>
  )
}
