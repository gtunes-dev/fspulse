import { useState, useEffect, useCallback } from 'react'
import { FolderTree, FolderOpen, Search } from 'lucide-react'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { RootPicker } from '@/components/shared/RootPicker'
import { CompactScanBar } from '@/components/shared/CompactScanBar'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { ItemDetailPanel } from '@/components/shared/ItemDetailPanel'
import { FileTreeView } from './FileTreeView'
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
  position: 'left' | 'right'
  defaultRootId?: string
}

export function BrowseCard({ roots, position, defaultRootId }: BrowseCardProps) {
  const [selectedRootId, setSelectedRootId] = useState<string>(defaultRootId ?? '')
  const [resolvedScanId, setResolvedScanId] = useState<number | null>(null)
  const [scanStatus, setScanStatus] = useState<'resolving' | 'resolved' | 'no-scan'>('resolving')
  const [viewMode, setViewMode] = useState<ViewMode>('tree')
  const [showDeleted, setShowDeleted] = useState(false)
  const [searchFilter, setSearchFilter] = useState('')
  const [debouncedSearch, setDebouncedSearch] = useState('')
  // Per-view selection: each tab has its own independently selected item
  const [selectedItems, setSelectedItems] = useState<Record<ViewMode, SelectedItem | null>>({
    tree: null,
    folder: null,
    search: null,
  })

  // Auto-select first root if no default provided
  useEffect(() => {
    if (!selectedRootId && roots.length > 0) {
      setSelectedRootId(roots[0].root_id.toString())
    }
  }, [roots, selectedRootId])

  const selectedRoot = roots.find(r => r.root_id.toString() === selectedRootId)

  // Reset resolved scan and all selections when root changes
  useEffect(() => {
    setResolvedScanId(null)
    setScanStatus('resolving')
    setSelectedItems({ tree: null, folder: null, search: null })
  }, [selectedRootId])

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

  const handleViewModeChange = (value: string) => {
    setViewMode(value as ViewMode)
  }

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
    <div className="flex-1 min-h-0 min-w-0 flex flex-col">
      {/* Tree View — always rendered */}
      <div className={viewMode === 'tree' ? 'border border-border rounded-lg flex-1 min-h-0 overflow-hidden' : 'hidden'}>
        <FileTreeView
          rootId={selectedRoot.root_id}
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          showDeleted={showDeleted}
          isActive={viewMode === 'tree'}
          selectedItemId={selectedItems.tree?.itemId}
          onItemSelect={handleTreeSelect}
        />
      </div>

      {/* Folder View — always rendered */}
      <div className={viewMode === 'folder' ? 'border border-border rounded-lg flex-1 min-h-0 overflow-hidden' : 'hidden'}>
        <FolderView
          rootId={selectedRoot.root_id}
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          showDeleted={showDeleted}
          isActive={viewMode === 'folder'}
          selectedItemId={selectedItems.folder?.itemId}
          onItemSelect={handleFolderSelect}
        />
      </div>

      {/* Search Results — always rendered */}
      <div className={viewMode === 'search' && hasSearchQuery ? 'border border-border rounded-lg flex-1 min-h-0 overflow-hidden' : 'hidden'}>
        <SearchResultsList
          rootId={selectedRoot.root_id}
          rootPath={selectedRoot.root_path}
          scanId={resolvedScanId}
          searchQuery={debouncedSearch}
          showDeleted={showDeleted}
          isActive={viewMode === 'search' && hasSearchQuery}
          selectedItemId={selectedItems.search?.itemId}
          onItemSelect={handleSearchSelect}
        />
      </div>

      {/* Search placeholder when no query */}
      {viewMode === 'search' && !hasSearchQuery && (
        <div className="border border-border rounded-lg flex-1 min-h-0">
          <div className="flex items-center justify-center h-full text-muted-foreground">
            Type a search query to find files and folders
          </div>
        </div>
      )}
    </div>
  ) : (
    <div className="border border-border rounded-lg flex-1 min-h-0 min-w-0">
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
    <div className="w-80 flex-shrink-0 border border-border rounded-lg">
      <ItemDetailPanel
        itemId={activeSelection.itemId}
        itemPath={activeSelection.itemPath}
        itemType={activeSelection.itemType}
        isTombstone={activeSelection.isTombstone}
        rootId={selectedRoot.root_id}
        scanId={resolvedScanId}
        onClose={handleDetailClose}
      />
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
            onScanResolved={handleScanResolved}
            onNoScan={handleNoScan}
          />
        )}

        {/* View mode tabs + controls */}
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

          <div className="flex items-center gap-2">
            <Checkbox
              id={`show-deleted-${position}`}
              checked={showDeleted}
              onCheckedChange={(checked) => setShowDeleted(checked === true)}
            />
            <Label htmlFor={`show-deleted-${position}`} className="text-sm font-medium cursor-pointer">
              Show deleted
            </Label>
          </div>
        </div>

        {/* Search filter — visible in search mode */}
        {viewMode === 'search' && (
          <SearchFilter
            value={searchFilter}
            onChange={handleSearchChange}
            placeholder="Search files and folders..."
          />
        )}

        {/* Content area: content view + detail panel side by side */}
        <div className="flex-1 flex gap-3">
          {/* For left card: content then detail. For right card: detail then content. */}
          {position === 'right' && detailPanel}
          {contentView}
          {position === 'left' && detailPanel}
        </div>
      </CardContent>
    </Card>
  )
}
