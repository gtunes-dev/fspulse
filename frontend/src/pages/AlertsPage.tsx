import { useState, useEffect } from 'react'
import { AlertsTab } from './insights/AlertsTab'
import { fetchQuery } from '@/lib/api'
import type { ContextFilterType, ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

export function AlertsPage() {
  const [contextFilter, setContextFilter] = useState<ContextFilterType>('all')
  const [contextValue, setContextValue] = useState('')
  const [roots, setRoots] = useState<Root[]>([])
  const [loading, setLoading] = useState(true)

  // Load roots for context filter
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
        console.error('Error loading roots:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRoots()
  }, [])

  const handleContextFilterChange = (value: ContextFilterType) => {
    setContextFilter(value)
    if (value === 'root' && roots.length > 0) {
      // Auto-select first root when switching to root context
      setContextValue(roots[0].root_id.toString())
    } else {
      setContextValue('')
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="text-muted-foreground">Loading...</div>
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-8">Alerts</h1>

      <div className="flex-1">
        <AlertsTab
          contextFilter={contextFilter}
          contextValue={contextValue}
          roots={roots}
          onContextFilterChange={handleContextFilterChange}
          onContextValueChange={setContextValue}
        />
      </div>
    </div>
  )
}
