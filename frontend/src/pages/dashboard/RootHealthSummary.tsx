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
import { Badge } from '@/components/ui/badge'
import {
  CheckCircle,
  XCircle,
  AlertTriangle,
  Clock,
  Loader2,
  FolderTree,
  Calendar,
  TrendingUp,
} from 'lucide-react'
import { formatDateRelative } from '@/lib/dateUtils'
import { countQuery } from '@/lib/api'
import { useTaskContext } from '@/contexts/TaskContext'
import type { RootWithScan } from '@/lib/types'

interface RootHealth extends RootWithScan {
  openAlertCount: number
  flaggedAlertCount: number
}

const scanStateIcon = (state: string) => {
  switch (state) {
    case 'Completed':
      return <CheckCircle className="h-4 w-4 text-green-500" />
    case 'Error':
      return <XCircle className="h-4 w-4 text-red-500" />
    case 'Stopped':
      return <AlertTriangle className="h-4 w-4 text-orange-500" />
    default:
      return <Loader2 className="h-4 w-4 text-blue-500 animate-spin" />
  }
}

const shortenPath = (path: string, maxLength: number = 40): string => {
  if (path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return `${parts[0]}/.../${parts[parts.length - 1]}`
}

export function RootHealthSummary() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [roots, setRoots] = useState<RootHealth[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    async function loadHealthData() {
      try {
        setLoading(true)

        // Fetch roots with last scan info
        const response = await fetch('/api/roots/with-scans')
        if (!response.ok) return
        const rootsData: RootWithScan[] = await response.json()

        // Fetch open and flagged alert counts for each root in parallel
        const healthData: RootHealth[] = await Promise.all(
          rootsData.map(async (root) => {
            const alertColumns = [{ name: 'alert_id', visible: true, sort_direction: 'none' as const, position: 0 }]
            const rootFilter = { column: 'root_id', value: String(root.root_id) }

            try {
              const [openResult, flaggedResult] = await Promise.all([
                countQuery('alerts', {
                  columns: alertColumns,
                  filters: [rootFilter, { column: 'alert_status', value: 'O' }],
                }),
                countQuery('alerts', {
                  columns: alertColumns,
                  filters: [rootFilter, { column: 'alert_status', value: 'F' }],
                }),
              ])
              return {
                ...root,
                openAlertCount: openResult.count,
                flaggedAlertCount: flaggedResult.count,
              }
            } catch {
              return { ...root, openAlertCount: 0, flaggedAlertCount: 0 }
            }
          })
        )

        setRoots(healthData)
      } catch (err) {
        console.error('Error loading root health data:', err)
      } finally {
        setLoading(false)
      }
    }

    loadHealthData()
  }, [lastTaskCompletedAt])

  if (loading && roots.length === 0) {
    return null
  }

  if (roots.length === 0) {
    return null
  }

  return (
    <Card>
      <CardHeader>
        <CardTitle>Root Health</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="border border-border rounded-lg overflow-hidden">
          <Table>
            <TableHeader className="bg-muted">
              <TableRow>
                <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                <TableHead className="uppercase text-xs tracking-wide">Last Scan</TableHead>
                <TableHead className="uppercase text-xs tracking-wide">Status</TableHead>
                <TableHead className="text-right uppercase text-xs tracking-wide">Alerts</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {roots.map((root) => (
                <TableRow key={root.root_id}>
                  <TableCell className="max-w-[300px]" title={root.root_path}>
                    <div className="flex items-center gap-2">
                      <span className="truncate font-medium">
                        {shortenPath(root.root_path)}
                      </span>
                      <div className="flex items-center gap-1 flex-shrink-0">
                        <Link
                          to={`/browse?root_id=${root.root_id}`}
                          className="text-muted-foreground hover:text-primary p-0.5 rounded"
                          title="Browse files"
                        >
                          <FolderTree className="h-4 w-4" />
                        </Link>
                        <Link
                          to={`/trends/scan-trends?root_id=${root.root_id}`}
                          className="text-muted-foreground hover:text-primary p-0.5 rounded"
                          title="View trends"
                        >
                          <TrendingUp className="h-4 w-4" />
                        </Link>
                        <Link
                          to={`/schedules?root_id=${root.root_id}`}
                          className="text-muted-foreground hover:text-primary p-0.5 rounded"
                          title="View schedules"
                        >
                          <Calendar className="h-4 w-4" />
                        </Link>
                      </div>
                    </div>
                  </TableCell>
                  <TableCell>
                    {root.last_scan ? (
                      <span className="flex items-center gap-1.5 text-sm">
                        <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                        {formatDateRelative(root.last_scan.started_at)}
                      </span>
                    ) : (
                      <span className="text-muted-foreground text-sm">Never scanned</span>
                    )}
                  </TableCell>
                  <TableCell>
                    {root.last_scan ? (
                      <span className="flex items-center gap-1.5">
                        {scanStateIcon(root.last_scan.state)}
                        <span className="text-sm">{root.last_scan.state}</span>
                      </span>
                    ) : (
                      <span className="text-muted-foreground">&mdash;</span>
                    )}
                  </TableCell>
                  <TableCell className="text-right">
                    <div className="flex items-center justify-end gap-2">
                      {root.openAlertCount > 0 ? (
                        <Link to={`/alerts?root_id=${root.root_id}&alert_status=O`} title="Open alerts">
                          <Badge variant="destructive">
                            {root.openAlertCount} open
                          </Badge>
                        </Link>
                      ) : null}
                      {root.flaggedAlertCount > 0 ? (
                        <Link to={`/alerts?root_id=${root.root_id}&alert_status=F`} title="Flagged alerts">
                          <Badge variant="outline" className="border-amber-500 text-amber-600 dark:text-amber-400">
                            {root.flaggedAlertCount} flagged
                          </Badge>
                        </Link>
                      ) : null}
                      {root.openAlertCount === 0 && root.flaggedAlertCount === 0 && (
                        <span className="text-muted-foreground text-sm">None</span>
                      )}
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
