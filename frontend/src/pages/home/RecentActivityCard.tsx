import { useState, useEffect } from 'react'
import { Link } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
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
import { shortenPath } from '@/lib/pathUtils'
import { CheckCircle, XCircle, AlertTriangle, CircleX, ArrowRight, Hash } from 'lucide-react'

interface TaskHistoryRow {
  task_id: number
  task_type: string
  root_id: number | null
  root_path: string | null
  status: string
  started_at: number | null
  completed_at: number | null
  scan_id: number | null
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  new_val_invalid_count: number | null
  new_hash_suspect_count: number | null
}

const RECENT_LIMIT = 5

const taskTypeDisplayName = (taskType: string): string => {
  switch (taskType) {
    case 'scan': return 'Scan'
    case 'compact_database': return 'Compact Database'
    default: return taskType
  }
}

export function RecentActivityCard() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [tasks, setTasks] = useState<TaskHistoryRow[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    async function loadRecentTasks() {
      try {
        const params = new URLSearchParams({
          limit: RECENT_LIMIT.toString(),
          offset: '0',
        })

        const response = await fetch(`/api/tasks/history/fetch?${params}`)
        if (response.ok) {
          const data = await response.json()
          setTasks(data.tasks)
        }
      } catch (err) {
        console.error('Error loading recent activity:', err)
      } finally {
        setLoading(false)
      }
    }

    loadRecentTasks()
  }, [lastTaskCompletedAt])

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

  if (loading && tasks.length === 0) {
    return null
  }

  if (tasks.length === 0) {
    return null
  }

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle>Recent Activity</CardTitle>
        <Link
          to="/history"
          className="text-sm text-muted-foreground hover:text-primary flex items-center gap-1"
        >
          View all history
          <ArrowRight className="h-4 w-4" />
        </Link>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="border border-border rounded-lg overflow-hidden">
          <Table>
            <TableHeader className="bg-muted">
              <TableRow>
                <TableHead className="uppercase text-xs tracking-wide">When</TableHead>
                <TableHead className="uppercase text-xs tracking-wide">Task</TableHead>
                <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                <TableHead className="text-center uppercase text-xs tracking-wide">Changes</TableHead>
                <TableHead className="text-center uppercase text-xs tracking-wide">Integrity</TableHead>
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
                  <TableCell className="max-w-[200px] truncate" title={task.root_path ?? undefined}>
                    {task.root_path ? shortenPath(task.root_path) : <span className="text-muted-foreground">&mdash;</span>}
                  </TableCell>
                  {/* Changes */}
                  <TableCell className="text-center text-sm">
                    {task.task_type === 'scan' && (task.add_count || task.modify_count || task.delete_count) ? (
                      <span className="inline-flex items-center gap-1.5">
                        {task.add_count ? <span className="text-green-500">+{task.add_count}</span> : null}
                        {task.modify_count ? <span className="text-blue-500">~{task.modify_count}</span> : null}
                        {task.delete_count ? <span className="text-red-500">-{task.delete_count}</span> : null}
                      </span>
                    ) : (
                      <span className="text-muted-foreground">&mdash;</span>
                    )}
                  </TableCell>

                  {/* Integrity */}
                  <TableCell className="text-center">
                    {task.task_type === 'scan' && (task.new_val_invalid_count || task.new_hash_suspect_count) ? (
                      <Link
                        to={`/integrity?root_id=${task.root_id}`}
                        className="inline-flex items-center gap-2.5 text-sm hover:underline"
                      >
                        {task.new_val_invalid_count ? (
                          <span className="inline-flex items-center gap-1 text-rose-500">
                            <CircleX className="h-3.5 w-3.5" />
                            {task.new_val_invalid_count}
                          </span>
                        ) : null}
                        {task.new_hash_suspect_count ? (
                          <span className="inline-flex items-center gap-1 text-amber-500">
                            <Hash className="h-3.5 w-3.5" />
                            {task.new_hash_suspect_count}
                          </span>
                        ) : null}
                      </Link>
                    ) : (
                      <span className="text-muted-foreground">&mdash;</span>
                    )}
                  </TableCell>

                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-2">
                      {getStatusIcon(task.status)}
                    </div>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
        </div>
      </CardContent>
    </Card>
  )
}
