import { useState, useEffect, useRef } from 'react'
import { Link } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { formatTimeAgo } from '@/lib/dateUtils'
import { useTaskContext } from '@/contexts/TaskContext'
import { CheckCircle, XCircle, AlertTriangle } from 'lucide-react'
import { RootDetailSheet } from '@/components/shared/RootDetailSheet'
import { TaskTypeFilter } from '@/components/shared/TaskTypeFilter'

interface TaskHistoryRow {
  task_id: number
  task_type: string       // "scan", "compact_database"
  root_id: number | null
  root_path: string | null
  schedule_name: string | null
  source: string          // "Manual", "Scheduled"
  status: string          // "completed", "stopped", "error"
  started_at: number | null
  completed_at: number | null
  scan_id: number | null
}

const ITEMS_PER_PAGE = 25

// Map task_type serde values to display names
const taskTypeDisplayName = (taskType: string): string => {
  switch (taskType) {
    case 'scan': return 'Scan'
    case 'compact_database': return 'Compact Database'
    default: return taskType
  }
}

export function TaskHistoryTable() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [tasks, setTasks] = useState<TaskHistoryRow[]>([])
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const [selectedRoot, setSelectedRoot] = useState<{ id: number; path: string } | null>(null)
  const [rootSheetOpen, setRootSheetOpen] = useState(false)
  const [selectedType, setSelectedType] = useState<string>('all')
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)
  const isInitialLoad = useRef(true)
  const lastFilterKeyRef = useRef<string>('')

  // Load task history
  useEffect(() => {
    async function loadTaskHistory() {
      try {
        // Only show loading on initial mount, keep old data during refetch
        if (isInitialLoad.current) {
          setLoading(true)
          isInitialLoad.current = false
        }
        setError(null)

        // Build filter key to detect when filters change
        const taskType = selectedType === 'all' ? undefined : selectedType
        const filterKey = `${taskType || 'all'}`
        const needsCount = filterKey !== lastFilterKeyRef.current

        // Get count only when filters change
        if (needsCount) {
          const countParams = new URLSearchParams()
          if (taskType) {
            countParams.append('task_type', taskType)
          }

          const countResponse = await fetch(`/api/tasks/history/count?${countParams}`)
          if (!countResponse.ok) {
            throw new Error(`Count query failed: ${countResponse.statusText}`)
          }

          const countData = await countResponse.json()
          setTotalCount(countData.count)
          lastFilterKeyRef.current = filterKey
        }

        // Always fetch current page
        const fetchParams = new URLSearchParams({
          limit: ITEMS_PER_PAGE.toString(),
          offset: ((currentPage - 1) * ITEMS_PER_PAGE).toString(),
        })
        if (taskType) {
          fetchParams.append('task_type', taskType)
        }

        const fetchResponse = await fetch(`/api/tasks/history/fetch?${fetchParams}`)
        if (!fetchResponse.ok) {
          throw new Error(`Fetch query failed: ${fetchResponse.statusText}`)
        }

        const fetchData = await fetchResponse.json()
        setTasks(fetchData.tasks)
      } catch (err) {
        console.error('Error loading task history:', err)
        setError(err instanceof Error ? err.message : 'Failed to load task history')
      } finally {
        setLoading(false)
      }
    }

    loadTaskHistory()
  }, [lastTaskCompletedAt, selectedType, currentPage])

  // Reset to page 1 when filters change
  useEffect(() => {
    setCurrentPage(1)
  }, [selectedType])

  // Format duration
  const formatDuration = (task: TaskHistoryRow): string => {
    if (!task.started_at || !task.completed_at) {
      return '-'
    }

    const durationSeconds = task.completed_at - task.started_at

    const hours = Math.floor(durationSeconds / 3600)
    const minutes = Math.floor((durationSeconds % 3600) / 60)
    const seconds = durationSeconds % 60

    if (hours > 0) {
      return `${hours}h ${minutes}m ${seconds}s`
    } else if (minutes > 0) {
      return `${minutes}m ${seconds}s`
    } else {
      return `${seconds}s`
    }
  }

  // Get status icon
  const getStatusIcon = (status: string) => {
    switch (status) {
      case 'completed':
        return <CheckCircle className="h-4 w-4 text-green-500" />
      case 'error':
        return <XCircle className="h-4 w-4 text-red-500" />
      case 'stopped':
        return <AlertTriangle className="h-4 w-4 text-orange-500" />
      default:
        return null
    }
  }

  // Get status text
  const getStatusText = (status: string) => {
    switch (status) {
      case 'completed':
        return 'Completed'
      case 'error':
        return 'Error'
      case 'stopped':
        return 'Stopped'
      default:
        return status
    }
  }

  // Shorten path
  const shortenPath = (path: string, maxLength: number = 30): string => {
    if (path.length <= maxLength) return path
    const parts = path.split('/')
    if (parts.length <= 2) return path

    return `${parts[0]}/.../${parts[parts.length - 1]}`
  }

  // Pagination
  const start = (currentPage - 1) * ITEMS_PER_PAGE + 1
  const end = Math.min(start + ITEMS_PER_PAGE - 1, totalCount)

  if (loading && tasks.length === 0) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Task History</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-muted-foreground text-center py-4">
            Loading task history...
          </p>
        </CardContent>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Task History</CardTitle>
        </CardHeader>
        <CardContent className="pt-6">
          <p className="text-sm text-red-500 text-center py-4">
            Error: {error}
          </p>
        </CardContent>
      </Card>
    )
  }

  if (tasks.length === 0 && !loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Task History</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Type Filter */}
          <TaskTypeFilter
            selectedType={selectedType}
            onTypeChange={setSelectedType}
          />

          <div className="border border-border rounded-lg">
            <p className="text-sm text-muted-foreground text-center py-12">
              {selectedType === 'all'
                ? 'No completed tasks yet'
                : 'No completed tasks of this type'}
            </p>
          </div>
        </CardContent>
      </Card>
    )
  }

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Task History</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Type Filter */}
          <TaskTypeFilter
            selectedType={selectedType}
            onTypeChange={setSelectedType}
          />

          {/* Bordered Table */}
          <div className="border border-border rounded-lg overflow-hidden">
            <Table>
              <TableHeader className="bg-muted">
                <TableRow>
                  <TableHead className="uppercase text-xs tracking-wide">Started</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Task</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide w-[120px]">Duration</TableHead>
                  <TableHead className="text-right uppercase text-xs tracking-wide">Status</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {tasks.map((task) => (
                  <TableRow key={task.task_id}>
                    <TableCell className="font-medium">
                      {task.started_at ? formatTimeAgo(task.started_at) : '-'}
                    </TableCell>
                    <TableCell>
                      {task.task_type === 'scan' && task.scan_id != null && task.root_id != null ? (
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
                    <TableCell
                      className="max-w-[200px] truncate"
                      title={task.root_path ?? undefined}
                    >
                      {task.root_path ? (
                        <button
                          onClick={(e) => {
                            e.stopPropagation()
                            setSelectedRoot({
                              id: task.root_id!,
                              path: task.root_path!
                            })
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
                    <TableCell>
                      {formatDuration(task)}
                    </TableCell>
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
        </CardContent>
      </Card>

      {/* Root Detail Sheet */}
      {selectedRoot && (
        <RootDetailSheet
          rootId={selectedRoot.id}
          rootPath={selectedRoot.path}
          open={rootSheetOpen}
          onOpenChange={setRootSheetOpen}
        />
      )}
    </>
  )
}
