import { useState, useEffect, useCallback, useRef } from 'react'
import { Link, useSearchParams } from 'react-router-dom'
import { Button } from '@/components/ui/button'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatDateTimeShort } from '@/lib/dateUtils'
import { useTaskContext } from '@/contexts/TaskContext'
import { CheckCircle, XCircle, AlertTriangle, Calendar, Play } from 'lucide-react'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'
import { RootCard } from '@/components/shared/RootCard'
import { TaskTypeFilter } from '@/components/shared/TaskTypeFilter'
import { fetchQuery } from '@/lib/api'
import { shortenPath } from '@/lib/pathUtils'
import type { ColumnSpec } from '@/lib/types'

interface HistoryRow {
  task_id: number
  task_type: string
  root_id: number | null
  root_path: string | null
  schedule_name: string | null
  source: string
  status: string
  started_at: number | null
  completed_at: number | null
  scan_id: number | null
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  was_restarted: boolean | null
}

interface Root {
  root_id: number
  root_path: string
}

const ITEMS_PER_PAGE = 25

const taskTypeDisplayName = (taskType: string): string => {
  switch (taskType) {
    case 'scan': return 'Scan'
    case 'compact_database': return 'Compact Database'
    default: return taskType
  }
}

export function HistoryPage() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [searchParams, setSearchParams] = useSearchParams()

  const initialRootId = searchParams.get('root_id') || 'all'
  const [selectedRootId, setSelectedRootId] = useState<string>(initialRootId)
  const [selectedType, setSelectedType] = useState<string>('all')

  const [roots, setRoots] = useState<Root[]>([])
  const [tasks, setTasks] = useState<HistoryRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)

  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)

  const isInitialLoad = useRef(true)
  const lastFilterKeyRef = useRef<string>('')

  // URL sync for root_id, reset page on filter change
  const handleRootChange = useCallback((rootId: string) => {
    setSelectedRootId(rootId)
    setCurrentPage(1)
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

  const handleTypeChange = useCallback((type: string) => {
    setSelectedType(type)
    setCurrentPage(1)
  }, [])

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

  // Load task history
  useEffect(() => {
    async function loadHistory() {
      try {
        if (isInitialLoad.current) {
          setLoading(true)
          isInitialLoad.current = false
        }
        setError(null)

        const rootId = selectedRootId === 'all' ? undefined : selectedRootId
        const taskType = selectedType === 'all' ? undefined : selectedType
        const filterKey = `${rootId || 'all'}-${taskType || 'all'}-${lastTaskCompletedAt}`
        const needsCount = filterKey !== lastFilterKeyRef.current

        if (needsCount) {
          const countParams = new URLSearchParams()
          if (rootId) countParams.append('root_id', rootId)
          if (taskType) countParams.append('task_type', taskType)

          const countResponse = await fetch(`/api/tasks/history/count?${countParams}`)
          if (!countResponse.ok) throw new Error(`Count query failed: ${countResponse.statusText}`)

          const countData = await countResponse.json()
          setTotalCount(countData.count)
          lastFilterKeyRef.current = filterKey
        }

        const fetchParams = new URLSearchParams({
          limit: ITEMS_PER_PAGE.toString(),
          offset: ((currentPage - 1) * ITEMS_PER_PAGE).toString(),
        })
        if (rootId) fetchParams.append('root_id', rootId)
        if (taskType) fetchParams.append('task_type', taskType)

        const fetchResponse = await fetch(`/api/tasks/history/fetch?${fetchParams}`)
        if (!fetchResponse.ok) throw new Error(`Fetch query failed: ${fetchResponse.statusText}`)

        const fetchData = await fetchResponse.json()
        setTasks(fetchData.tasks)
      } catch (err) {
        console.error('Error loading history:', err)
        setError(err instanceof Error ? err.message : 'Failed to load history')
      } finally {
        setLoading(false)
      }
    }

    loadHistory()
  }, [lastTaskCompletedAt, selectedRootId, selectedType, currentPage])

  // Helpers
  const formatChanges = (add: number | null, modify: number | null, del: number | null): string => {
    const changes = []
    if (add && add > 0) changes.push(`${add} ${add === 1 ? 'add' : 'adds'}`)
    if (modify && modify > 0) changes.push(`${modify} ${modify === 1 ? 'mod' : 'mods'}`)
    if (del && del > 0) changes.push(`${del} ${del === 1 ? 'del' : 'dels'}`)
    if (changes.length === 0) return 'No changes'
    return changes.join(', ')
  }

  const formatDuration = (task: HistoryRow): string => {
    if (task.was_restarted) return '\u2014'
    if (!task.started_at || !task.completed_at) return '\u2014'

    const durationSeconds = task.completed_at - task.started_at
    const hours = Math.floor(durationSeconds / 3600)
    const minutes = Math.floor((durationSeconds % 3600) / 60)
    const seconds = durationSeconds % 60

    if (hours > 0) return `${hours}h ${minutes}m ${seconds}s`
    if (minutes > 0) return `${minutes}m ${seconds}s`
    return `${seconds}s`
  }

  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'completed': return <CheckCircle className="h-4 w-4 text-green-500" />
      case 'error': return <XCircle className="h-4 w-4 text-red-500" />
      case 'stopped': return <AlertTriangle className="h-4 w-4 text-orange-500" />
      default: return null
    }
  }

  const getStatusText = (status: string) => {
    switch (status) {
      case 'completed': return 'Completed'
      case 'error': return 'Error'
      case 'stopped': return 'Stopped'
      default: return status
    }
  }

  const isScan = (task: HistoryRow) => task.task_type === 'scan'

  // Pagination
  const start = (currentPage - 1) * ITEMS_PER_PAGE + 1
  const end = Math.min(start + ITEMS_PER_PAGE - 1, totalCount)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">History</h1>

      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={handleRootChange}
        allowAll={true}
        actionBar={
          <TaskTypeFilter
            selectedType={selectedType}
            onTypeChange={handleTypeChange}
          />
        }
      >
        {loading && tasks.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-8">Loading history...</p>
        ) : error ? (
          <p className="text-sm text-red-500 text-center py-8">Error: {error}</p>
        ) : tasks.length === 0 ? (
          <div className="border border-border rounded-lg">
            <p className="text-sm text-muted-foreground text-center py-12">
              {selectedRootId === 'all' && selectedType === 'all'
                ? 'No completed tasks yet.'
                : 'No completed tasks matching these filters.'}
            </p>
          </div>
        ) : (
          <>
            <div className="border border-border rounded-lg overflow-hidden">
              <Table>
                <TableHeader className="bg-muted">
                  <TableRow>
                    <TableHead className="uppercase text-xs tracking-wide">Started</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide">Task</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide">Schedule</TableHead>
                    <TableHead className="uppercase text-xs tracking-wide w-[120px]">Duration</TableHead>
                    <TableHead className="text-center uppercase text-xs tracking-wide">Changes</TableHead>
                    <TableHead className="text-right uppercase text-xs tracking-wide">Status</TableHead>
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {tasks.map((task) => (
                    <TableRow key={task.task_id}>
                      {/* Started */}
                      <TableCell className="font-medium">
                        {task.started_at ? formatDateTimeShort(task.started_at) : '\u2014'}
                      </TableCell>

                      {/* Task */}
                      <TableCell>
                        {isScan(task) && task.scan_id != null && task.root_id != null ? (
                          <Link
                            to={`/browse?root_id=${task.root_id}&scan_id=${task.scan_id}`}
                            className="hover:underline hover:text-primary"
                          >
                            Scan #{task.scan_id}
                          </Link>
                        ) : (
                          taskTypeDisplayName(task.task_type)
                        )}
                      </TableCell>

                      {/* Root */}
                      <TableCell
                        className="max-w-[200px] truncate"
                        title={task.root_path ?? undefined}
                      >
                        {task.root_path ? (
                          <button
                            onClick={(e) => {
                              e.stopPropagation()
                              setSelectedRoot({ id: task.root_id!, path: task.root_path! })
                              setRootSheetOpen(true)
                            }}
                            className="text-left hover:underline hover:text-primary cursor-pointer"
                          >
                            {shortenPath(task.root_path)}
                          </button>
                        ) : (
                          <span className="text-muted-foreground">&mdash;</span>
                        )}
                      </TableCell>

                      {/* Schedule */}
                      <TableCell>
                        {isScan(task) ? (
                          task.schedule_name ? (
                            <div className="flex items-center gap-2">
                              <Calendar className="h-4 w-4 text-blue-500" />
                              <span>{task.schedule_name}</span>
                            </div>
                          ) : (
                            <div className="flex items-center gap-2">
                              <Play className="h-4 w-4 text-blue-500" />
                              <span>Manual</span>
                            </div>
                          )
                        ) : (
                          <span className="text-muted-foreground">&mdash;</span>
                        )}
                      </TableCell>

                      {/* Duration */}
                      <TableCell>
                        {formatDuration(task)}
                      </TableCell>

                      {/* Changes */}
                      <TableCell className="text-center">
                        {isScan(task) ? (
                          formatChanges(task.add_count, task.modify_count, task.delete_count)
                        ) : (
                          <span className="text-muted-foreground">&mdash;</span>
                        )}
                      </TableCell>

                      {/* Status */}
                      <TableCell className="text-right">
                        <div className="flex items-center justify-end gap-2">
                          {getStatusIcon(task.status)}
                          <span className="text-sm">{getStatusText(task.status)}</span>
                        </div>
                      </TableCell>
                    </TableRow>
                  ))}
                </TableBody>
              </Table>
            </div>

            {/* Pagination */}
            {totalCount > ITEMS_PER_PAGE && (
              <div className="flex items-center justify-between">
                <div className="text-sm text-muted-foreground whitespace-nowrap">
                  Showing {(totalCount > 0 ? start : 0).toLocaleString()} - {end.toLocaleString()} of {totalCount.toLocaleString()} tasks
                </div>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setCurrentPage((prev) => prev - 1)}
                    disabled={currentPage === 1 || loading}
                  >
                    Previous
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => setCurrentPage((prev) => prev + 1)}
                    disabled={end >= totalCount || loading}
                  >
                    Next
                  </Button>
                </div>
              </div>
            )}
          </>
        )}
      </RootCard>

      {/* Root Detail Sheet */}
      {selectedRoot && (
        <RootDetailSheet
          rootId={selectedRoot.id}
          rootPath={selectedRoot.path}
          open={rootSheetOpen}
          onOpenChange={setRootSheetOpen}
        />
      )}
    </div>
  )
}
