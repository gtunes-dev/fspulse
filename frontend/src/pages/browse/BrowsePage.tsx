import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { BrowseCard } from './BrowseCard'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { selectionFromSearch } from '@/lib/timeRange'

interface Root {
  root_id: number
  root_path: string
}

interface BrowsePageProps {
  isActive?: boolean
}

export function BrowsePage({ isActive = true }: BrowsePageProps) {
  const [searchParams] = useSearchParams()
  const [roots, setRoots] = useState<Root[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const defaultRootId = searchParams.get('root_id') ?? undefined
  // A link may carry a time or range: ?from=<unix seconds>&to=<unix seconds>
  const defaultSelection = selectionFromSearch(searchParams)

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
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load roots')
        console.error('Error loading roots:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRoots()
  }, [])

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
          No roots configured. Add a root on the Roots page to get started.
        </div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full gap-4">
      {/* Page header */}
      <div className="flex items-center justify-between">
        <h1 className="text-2xl font-semibold">Browse</h1>
      </div>

      {/* Card container */}
      <div className="flex gap-4 flex-1 min-h-0">
        <BrowseCard
          roots={roots}
          defaultRootId={defaultRootId}
          defaultSelection={defaultSelection}
          isActive={isActive}
          isPrimary
        />
      </div>
    </div>
  )
}
