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
import {
  CheckCircle,
  XCircle,
  AlertTriangle,
  Clock,
  Loader2,
  FolderTree,
  ShieldAlert,
  Calendar,
  TrendingUp,
} from 'lucide-react'
import { formatDateRelative } from '@/lib/dateUtils'
import { shortenPath } from '@/lib/pathUtils'
import { useTaskContext } from '@/contexts/TaskContext'
import type { RootWithScan } from '@/lib/types'

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

export function RootHealthSummary() {
  const { lastTaskCompletedAt, currentTaskId } = useTaskContext()
  const [roots, setRoots] = useState<RootWithScan[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    async function loadHealthData() {
      try {
        setLoading(true)
        const response = await fetch('/api/roots/with-scans')
        if (!response.ok) return
        const rootsData: RootWithScan[] = await response.json()
        setRoots(rootsData)
      } catch (err) {
        console.error('Error loading root health data:', err)
      } finally {
        setLoading(false)
      }
    }

    loadHealthData()
  }, [lastTaskCompletedAt, currentTaskId])

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
                <TableHead className="text-right uppercase text-xs tracking-wide">Status</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {roots.map((root) => (
                <TableRow key={root.root_id}>
                  <TableCell className="max-w-[300px]" title={root.root_path}>
                    <div className="flex items-center gap-2">
                      <span className="truncate font-medium">
                        {shortenPath(root.root_path, 40)}
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
                          to={`/integrity?root_id=${root.root_id}`}
                          className="text-muted-foreground hover:text-primary p-0.5 rounded"
                          title="View integrity"
                        >
                          <ShieldAlert className="h-4 w-4" />
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
                  <TableCell className="text-right">
                    {root.last_scan ? (
                      <span className="inline-flex items-center gap-1.5">
                        {scanStateIcon(root.last_scan.state)}
                        <span className="text-sm">{root.last_scan.state}</span>
                      </span>
                    ) : (
                      <span className="text-muted-foreground">&mdash;</span>
                    )}
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
