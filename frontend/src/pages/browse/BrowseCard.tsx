import { useState, useEffect, useCallback, useRef } from 'react'
import { FolderTree, FolderOpen, Search, ArrowLeftRight, AlertTriangle, CircleX, Check, ChevronDown, SlidersHorizontal } from 'lucide-react'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { cn } from '@/lib/utils'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@/components/ui/collapsible'
import { RootPicker } from '@/components/shared/RootPicker'
import { CompactScanBar } from '@/components/shared/CompactScanBar'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { ItemDetailPanel } from '@/components/shared/ItemDetailPanel'
import { useBrowseCache } from '@/hooks/useBrowseCache'
import { getParentPath, type ChangeKind, type HashState, type ValState } from '@/lib/pathUtils'
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
}

export function BrowseCard({ roots, defaultRootId, defaultScanId }: BrowseCardProps) {
  const [selectedRootId, setSelectedRootId] = useState<string>(defaultRootId ?? '')
  const [resolvedScanId, setResolvedScanId] = useState<number | null>(null)
  const [scanStatus, setScanStatus] = useState<'resolving' | 'resolved' | 'no-scan'>('resolving')
  const [viewMode, setViewMode] = useState<ViewMode>('tree')
  const [hiddenKinds, setHiddenKinds] = useState<Set<ChangeKind>>(new Set())
  const [hiddenHashStates, setHiddenHashStates] = useState<Set<HashState>>(new Set())
  const [hiddenValStates, setHiddenValStates] = useState<Set<ValState>>(new Set())
  const [filtersOpen, setFiltersOpen] = useState(true)
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

  // Auto-select first root if no default provided
  useEffect(() => {
    if (!selectedRootId && roots.length > 0) {
      setSelectedRootId(roots[0].root_id.toString())
    }
  }, [roots, selectedRootId])

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
          hiddenHashStates={hiddenHashStates}
          hiddenValStates={hiddenValStates}
          isActive={viewMode === 'tree'}
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
          hiddenHashStates={hiddenHashStates}
          hiddenValStates={hiddenValStates}
          isActive={viewMode === 'folder'}
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
          hiddenHashStates={hiddenHashStates}
          hiddenValStates={hiddenValStates}
          isActive={viewMode === 'search' && hasSearchQuery}
          selectedItemId={selectedItems.search?.itemId}
          onItemSelect={handleSearchSelect}
        />
      </div>

      {/* Search placeholder when no query */}
      {viewMode === 'search' && !hasSearchQuery && (
        <div className="flex-1 min-h-0">
          <div className="flex items-center justify-center h-full text-muted-foreground">
            Type a search query to find files and folders
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
      "w-96 flex-shrink-0",
      detailOnRight ? 'border-l border-border' : 'border-r border-border'
    )}>
      <div className="sticky top-0">
        <ItemDetailPanel
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

        {/* View mode tabs + layout control */}
        <div className="flex items-center gap-3">
          <Tabs value={viewMode} onValueChange={handleViewModeChange}>
            <TabsList>
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

          <div className="flex-1" />

          <button
            className="p-1 rounded hover:bg-accent transition-colors text-muted-foreground hover:text-foreground"
            onClick={() => setDetailOnRight(prev => !prev)}
            aria-label="Flip detail panel side"
          >
            <ArrowLeftRight className="h-3.5 w-3.5" />
          </button>
        </div>

        {/* Collapsible filter panel */}
        <Collapsible open={filtersOpen} onOpenChange={setFiltersOpen}>
          <div className="border border-border rounded-lg bg-muted/30">
            {/* Header row — always visible */}
            <div className="flex items-center px-3 py-1.5">
              <CollapsibleTrigger asChild>
                <button className="flex items-center gap-2 border-none bg-transparent p-0 cursor-pointer">
                  <ChevronDown className={cn("h-4 w-4 flex-shrink-0 text-muted-foreground transition-transform", !filtersOpen && "-rotate-90")} />
                  <SlidersHorizontal className="h-3.5 w-3.5 text-muted-foreground" />
                  <span className="text-sm font-medium">Filters</span>
                </button>
              </CollapsibleTrigger>
            </div>

            {/* Expandable content — three vertical columns */}
            <CollapsibleContent>
              <div className="flex gap-5 px-4 pb-3">
                {/* Change column */}
                <div>
                  <div className="text-xs font-medium text-muted-foreground border-b border-border pb-1 mb-1.5">Change Type</div>
                  <div className="flex flex-col gap-0.5">
                    {([
                      { kind: 'added' as ChangeKind, label: 'Added', color: 'bg-green-500', ring: 'ring-green-500/40' },
                      { kind: 'modified' as ChangeKind, label: 'Modified', color: 'bg-blue-500', ring: 'ring-blue-500/40' },
                      { kind: 'deleted' as ChangeKind, label: 'Deleted', color: 'bg-red-500', ring: 'ring-red-500/40' },
                      { kind: 'unchanged' as ChangeKind, label: 'Unchanged', color: 'bg-zinc-400', ring: 'ring-zinc-400/40' },
                    ]).map(({ kind, label, color, ring }) => {
                      const visible = !hiddenKinds.has(kind)
                      return (
                        <button
                          key={kind}
                          className={cn(
                            'inline-flex items-center gap-2 px-2 py-0.5 rounded text-sm cursor-pointer transition-colors text-left',
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
                          <span
                            className={cn(
                              'inline-block w-3 h-3 rounded-full transition-all flex-shrink-0',
                              visible
                                ? `${color} ring-2 ${ring}`
                                : 'bg-transparent ring-1 ring-muted-foreground/25'
                            )}
                          />
                          {label}
                        </button>
                      )
                    })}
                  </div>
                </div>

                <div className="w-px bg-border" />

                {/* Hash column */}
                <div>
                  <div className="text-xs font-medium text-muted-foreground border-b border-border pb-1 mb-1.5">Hash State</div>
                  <div className="flex flex-col gap-0.5">
                    {([
                      { state: 'suspect' as HashState, label: 'Suspect', icon: AlertTriangle },
                      { state: 'unknown' as HashState, label: 'Unknown', icon: null },
                      { state: 'valid' as HashState, label: 'Valid', icon: null },
                    ] as const).map(({ state, label, icon: Icon }) => {
                      const visible = !hiddenHashStates.has(state)
                      return (
                        <button
                          key={state}
                          className={cn(
                            'inline-flex items-center gap-2 px-2 py-0.5 rounded text-sm cursor-pointer transition-colors text-left',
                            visible
                              ? 'text-foreground hover:bg-accent'
                              : 'text-muted-foreground/40 hover:bg-accent/50'
                          )}
                          onClick={() => setHiddenHashStates(prev => {
                            const next = new Set(prev)
                            if (next.has(state)) next.delete(state)
                            else next.add(state)
                            return next
                          })}
                        >
                          <span className={cn(
                            'inline-flex items-center justify-center w-4 h-4 rounded border transition-all flex-shrink-0',
                            visible
                              ? 'border-foreground/50 bg-foreground/10'
                              : 'border-muted-foreground/25 bg-transparent'
                          )}>
                            {visible && <Check className="h-3 w-3" />}
                          </span>
                          {label}
                          {Icon && <Icon className={cn('h-3.5 w-3.5', visible ? 'text-amber-500' : 'text-muted-foreground/30')} />}
                        </button>
                      )
                    })}
                  </div>
                </div>

                <div className="w-px bg-border" />

                {/* Validation column */}
                <div>
                  <div className="text-xs font-medium text-muted-foreground border-b border-border pb-1 mb-1.5">Validation State</div>
                  <div className="flex flex-col gap-0.5">
                    {([
                      { state: 'invalid' as ValState, label: 'Invalid', icon: CircleX },
                      { state: 'unknown' as ValState, label: 'Unknown', icon: null },
                      { state: 'valid' as ValState, label: 'Valid', icon: null },
                      { state: 'no_validator' as ValState, label: 'No Validator', icon: null },
                    ] as const).map(({ state, label, icon: Icon }) => {
                      const visible = !hiddenValStates.has(state)
                      return (
                        <button
                          key={state}
                          className={cn(
                            'inline-flex items-center gap-2 px-2 py-0.5 rounded text-sm cursor-pointer transition-colors text-left',
                            visible
                              ? 'text-foreground hover:bg-accent'
                              : 'text-muted-foreground/40 hover:bg-accent/50'
                          )}
                          onClick={() => setHiddenValStates(prev => {
                            const next = new Set(prev)
                            if (next.has(state)) next.delete(state)
                            else next.add(state)
                            return next
                          })}
                        >
                          <span className={cn(
                            'inline-flex items-center justify-center w-4 h-4 rounded border transition-all flex-shrink-0',
                            visible
                              ? 'border-foreground/50 bg-foreground/10'
                              : 'border-muted-foreground/25 bg-transparent'
                          )}>
                            {visible && <Check className="h-3 w-3" />}
                          </span>
                          {label}
                          {Icon && <Icon className={cn('h-3.5 w-3.5', visible ? 'text-rose-500' : 'text-muted-foreground/30')} />}
                        </button>
                      )
                    })}
                  </div>
                </div>
              </div>
            </CollapsibleContent>
          </div>
        </Collapsible>

        {/* Search filter — visible in search mode */}
        {viewMode === 'search' && (
          <SearchFilter
            value={searchFilter}
            onChange={handleSearchChange}
            placeholder="Search files and folders..."
          />
        )}

        {/* Content area: content view + detail panel side by side */}
        <div className="flex-1 flex border border-border rounded-lg">
          {!detailOnRight && detailPanel}
          {contentView}
          {detailOnRight && detailPanel}
        </div>
      </CardContent>
    </Card>
  )
}
