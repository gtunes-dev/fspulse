import { useState, useEffect, useCallback } from 'react'
import { Checkbox } from '@/components/ui/checkbox'
import { Label } from '@/components/ui/label'
import { RootCard } from '@/components/shared/RootCard'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { FileTreeView } from './FileTreeView'
import { ScanDatePicker } from './ScanDatePicker'
import { OldFileTreeView } from './OldFileTreeView'
import { OldSearchResultsList } from './OldSearchResultsList'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

export function BrowsePage() {
  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  // New temporal card state
  const [resolvedScanId, setResolvedScanId] = useState<number | null>(null)
  const [showDeleted, setShowDeleted] = useState(false)

  // Old card state
  const [showTombstones, setShowTombstones] = useState(false)
  const [searchFilter, setSearchFilter] = useState('')
  const [debouncedSearch, setDebouncedSearch] = useState('')

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
        setLoading(true)
        const columns: ColumnSpec[] = [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'asc', position: 1 },
        ]

        const response = await fetchQuery('roots', {
          columns,
          filters: [],
          limit: 1000,
          offset: 0,
        })

        const rootsData: Root[] = response.rows.map((row) => ({
          root_id: parseInt(row[0]),
          root_path: row[1],
        }))

        setRoots(rootsData)

        // Auto-select first root
        if (rootsData.length > 0) {
          setSelectedRootId(rootsData[0].root_id.toString())
        }
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load roots')
        console.error('Error loading roots:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRoots()
  }, [])

  const selectedRoot = roots.find(r => r.root_id.toString() === selectedRootId)

  // Reset resolved scan when root changes
  useEffect(() => {
    setResolvedScanId(null)
  }, [selectedRootId])

  // Stable callbacks for ScanDatePicker
  const handleScanResolved = useCallback((scanId: number) => {
    setResolvedScanId(scanId)
  }, [])

  const handleNoScan = useCallback(() => {
    setResolvedScanId(null)
  }, [])

  // Handle search filter with debouncing (old card)
  const handleSearchChange = (value: string) => {
    setSearchFilter(value)
  }

  // Debounce search input (old card)
  useEffect(() => {
    const timeout = setTimeout(() => {
      setDebouncedSearch(searchFilter)
    }, 300)

    return () => clearTimeout(timeout)
  }, [searchFilter])

  const hasSearchQuery = debouncedSearch.trim().length > 0

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-muted-foreground">Loading roots...</div>
      </div>
    )
  }

  if (error) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-red-600">{error}</div>
      </div>
    )
  }

  if (roots.length === 0) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-muted-foreground">
          No roots configured. Add a root on the Scan page to get started.
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full gap-6">
      <h1 className="text-2xl font-semibold mb-2">Browse</h1>

      {/* New temporal browse card */}
      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={setSelectedRootId}
        actionBar={
          <>
            {selectedRoot && (
              <ScanDatePicker
                rootId={selectedRoot.root_id}
                onScanResolved={handleScanResolved}
                onNoScan={handleNoScan}
              />
            )}

            <div className="flex items-center gap-2">
              <Checkbox
                id="show-deleted-new"
                checked={showDeleted}
                onCheckedChange={(checked) => setShowDeleted(checked === true)}
              />
              <Label htmlFor="show-deleted-new" className="text-sm font-medium cursor-pointer">
                Show deleted
              </Label>
            </div>
          </>
        }
      >
        <div className="border border-border rounded-lg">
          {selectedRoot && resolvedScanId ? (
            <FileTreeView
              rootId={selectedRoot.root_id}
              rootPath={selectedRoot.root_path}
              scanId={resolvedScanId}
              showDeleted={showDeleted}
            />
          ) : (
            <div className="flex items-center justify-center h-64 text-muted-foreground">
              {selectedRoot ? 'Resolving scan...' : 'Select a root'}
            </div>
          )}
        </div>
      </RootCard>

      {/* Old browse card (to be removed at cutover) */}
      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={setSelectedRootId}
        actionBar={
          <>
            <SearchFilter
              value={searchFilter}
              onChange={handleSearchChange}
            />

            <div className="flex items-center gap-2">
              <Checkbox
                id="show-deleted-old"
                checked={showTombstones}
                onCheckedChange={(checked) => setShowTombstones(checked === true)}
              />
              <Label htmlFor="show-deleted-old" className="text-sm font-medium cursor-pointer">
                Show deleted
              </Label>
            </div>
          </>
        }
      >
        {/* Tree View - hidden when searching */}
        <div style={{ display: hasSearchQuery ? 'none' : 'block' }}>
          <div className="border border-border rounded-lg">
            {selectedRoot && (
              <OldFileTreeView
                rootId={selectedRoot.root_id}
                rootPath={selectedRoot.root_path}
                showTombstones={showTombstones}
              />
            )}
          </div>
        </div>

        {/* Search Results - shown when searching */}
        {hasSearchQuery && selectedRoot && (
          <div className="border border-border rounded-lg">
            <OldSearchResultsList
              rootId={selectedRoot.root_id}
              searchQuery={debouncedSearch}
              showTombstones={showTombstones}
            />
          </div>
        )}
      </RootCard>
    </div>
  )
}
