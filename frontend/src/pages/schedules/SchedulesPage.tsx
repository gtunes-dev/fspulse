import { useState, useEffect, useCallback, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'
import { useTaskContext } from '@/contexts/TaskContext'
import { RootCard } from '@/components/shared/RootCard'
import { Button } from '@/components/ui/button'
import { SchedulesTable, type SchedulesTableRef } from '../setup/SchedulesTable'
import { CreateScheduleDialog } from '../setup/CreateScheduleDialog'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

export function SchedulesPage() {
  const { isRunning } = useTaskContext()
  const schedulesTableRef = useRef<SchedulesTableRef>(null)
  const [searchParams, setSearchParams] = useSearchParams()
  const [createDialogOpen, setCreateDialogOpen] = useState(false)
  const [roots, setRoots] = useState<Root[]>([])

  // Read root_id from URL for pre-filtering
  const initialRootId = searchParams.get('root_id') || 'all'
  const [selectedRootId, setSelectedRootId] = useState<string>(initialRootId)

  const preselectedRootId = selectedRootId !== 'all' ? parseInt(selectedRootId) : undefined

  // Update URL when root changes so sidebar can carry it to other pages
  const handleRootChange = useCallback((rootId: string) => {
    setSelectedRootId(rootId)
    if (rootId && rootId !== 'all') {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev)
        next.set('root_id', rootId)
        return next
      }, { replace: true })
    } else {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev)
        next.delete('root_id')
        return next
      }, { replace: true })
    }
  }, [setSearchParams])

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
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
      }
    }

    loadRoots()
  }, [])

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Schedules</h1>

      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={handleRootChange}
        allowAll={true}
        actionBar={
          <Button onClick={() => setCreateDialogOpen(true)} size="default">
            Add Schedule
          </Button>
        }
      >
        <SchedulesTable
          ref={schedulesTableRef}
          isScanning={isRunning}
          selectedRootId={selectedRootId}
        />
      </RootCard>

      <CreateScheduleDialog
        open={createDialogOpen}
        onOpenChange={setCreateDialogOpen}
        preselectedRootId={preselectedRootId}
        onSuccess={() => schedulesTableRef.current?.reload()}
      />
    </div>
  )
}
