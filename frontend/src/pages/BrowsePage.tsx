import { useState, useEffect } from 'react'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
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
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
        setLoading(true)
        const columns: ColumnSpec[] = [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
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
      <h1 className="text-2xl font-semibold">Browse</h1>

      {/* Controls Section */}
      <div className="flex items-center justify-between gap-4">
        {/* Root Picker */}
        <div className="flex items-center gap-3">
          <span className="text-base font-medium text-muted-foreground">Root</span>
          <Select value={selectedRootId} onValueChange={setSelectedRootId}>
            <SelectTrigger className="h-auto border-none shadow-none px-0 text-xl font-semibold hover:bg-transparent focus:ring-0">
              <SelectValue>
                {selectedRoot ? selectedRoot.root_path : 'Select a root'}
              </SelectValue>
            </SelectTrigger>
            <SelectContent>
              {roots.map((root) => (
                <SelectItem key={root.root_id} value={root.root_id.toString()}>
                  {root.root_path}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        {/* Tombstone Toggle */}
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={showTombstones}
            onChange={(e) => setShowTombstones(e.target.checked)}
            className="cursor-pointer"
          />
          <span className="text-sm text-muted-foreground">Show deleted items</span>
        </label>
      </div>

      {/* File Tree */}
      {selectedRoot && (
        <FileTreeView
          rootId={selectedRoot.root_id}
          rootPath={selectedRoot.root_path}
          showTombstones={showTombstones}
        />
      )}
    </div>
  )
}
