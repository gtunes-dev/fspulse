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
import { useTaskContext } from '@/contexts/useTaskContext'
import type { RootWithScan } from '@/lib/types'
import { Badge } from '@/components/ui/badge'
import { formatTimeAgo } from '@/lib/dateUtils'
import { describeLastCheck, describeTracking, fetchObservation, type RootObservation } from '@/lib/observation'

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
  const { lastTaskCompletedAt, currentTaskId, activeRootId } = useTaskContext()
  const [roots, setRoots] = useState<RootWithScan[]>([])
  const [loading, setLoading] = useState(true)
  const [observations, setObservations] = useState<Map<number, RootObservation>>(new Map())

  // Change tracking status. Also on a timer: the watch state changes
  // without any task starting or finishing.
  useEffect(() => {
    let cancelled = false
    const load = () => {
      fetchObservation()
        .then((data) => {
          if (!cancelled) setObservations(new Map(data.map((o) => [o.root_id, o])))
        })
        .catch((err) => console.error('Error loading change tracking status:', err))
    }
    load()
    const timer = window.setInterval(load, 30_000)
    return () => {
      cancelled = true
      window.clearInterval(timer)
    }
  }, [lastTaskCompletedAt, currentTaskId])

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
                <TableHead className="uppercase text-xs tracking-wide">Change Tracking</TableHead>
                <TableHead className="uppercase text-xs tracking-wide">Last Integrity Check</TableHead>
                <TableHead className="text-right uppercase text-xs tracking-wide">Result</TableHead>
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
                  <TableCell className="text-sm">
                    {(() => {
                      const observation = observations.get(root.root_id)
                      if (!observation) return <span className="text-muted-foreground">Loading...</span>
                      const tracking = describeTracking(observation)
                      return (
                        <div className="flex flex-col gap-0.5" title={tracking.explanation ?? undefined}>
                          <div className="flex items-center gap-2">
                            {tracking.attention ? (
                              <Badge variant={tracking.attention}>{tracking.label}</Badge>
                            ) : (
                              <span>{tracking.label}</span>
                            )}
                            {observation.walk_block && (
                              <Link to="/roots" className="hover:underline">
                                <Badge variant="warning">Deletions not recorded</Badge>
                              </Link>
                            )}
                          </div>
                          <span className="text-xs text-muted-foreground">
                            {describeLastCheck(
                              observation,
                              activeRootId === root.root_id && !observation.last_walk_completed_at,
                              formatTimeAgo
                            )}
                          </span>
                        </div>
                      )
                    })()}
                  </TableCell>
                  <TableCell>
                    {root.last_scan ? (
                      <span className="flex items-center gap-1.5 text-sm">
                        <Clock className="h-3.5 w-3.5 text-muted-foreground" />
                        {formatDateRelative(root.last_scan.started_at)}
                      </span>
                    ) : (
                      <span className="text-muted-foreground text-sm">Never checked</span>
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
