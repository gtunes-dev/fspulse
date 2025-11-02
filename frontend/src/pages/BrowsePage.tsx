import { useState, useEffect } from 'react'
import { RootPicker } from '@/components/ui/RootPicker'
import { SearchFilter } from '@/components/ui/SearchFilter'
import { FilterToolbar } from '@/components/ui/FilterToolbar'
import { FileTreeView } from '@/components/browse/FileTreeView'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

export function BrowsePage() {
  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [showTombstones, setShowTombstones] = useState(false)
  const [searchFilter, setSearchFilter] = useState('')
  const [searchDebounce, setSearchDebounce] = useState<number | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

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

  // Handle search filter with debouncing
  const handleSearchChange = (value: string) => {
    setSearchFilter(value)
    if (searchDebounce) {
      clearTimeout(searchDebounce)
    }
    const timeout = setTimeout(() => {
      // Trigger reload via useEffect in FileTreeView
    }, 500)
    setSearchDebounce(timeout)
  }

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
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-8">Browse</h1>

      {/* Filter Controls */}
      <FilterToolbar>
        <RootPicker
          roots={roots}
          value={selectedRootId}
          onChange={setSelectedRootId}
        />

        <SearchFilter
          value={searchFilter}
          onChange={handleSearchChange}
        />

        <label className="flex items-center gap-2.5 px-3 py-2 rounded-lg hover:bg-muted/30 cursor-pointer transition-colors">
          <input
            type="checkbox"
            checked={showTombstones}
            onChange={(e) => setShowTombstones(e.target.checked)}
            className="cursor-pointer h-4 w-4 rounded border-border"
          />
          <span className="text-sm font-medium text-muted-foreground whitespace-nowrap">Show deleted</span>
        </label>
      </FilterToolbar>

      {/* File Tree - Content Area */}
      <div className="flex-1">
        {selectedRoot && (
          <FileTreeView
            rootId={selectedRoot.root_id}
            rootPath={selectedRoot.root_path}
            showTombstones={showTombstones}
            searchFilter={searchFilter}
          />
        )}
      </div>
    </div>
  )
}
