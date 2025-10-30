import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { formatTimeAgo } from '@/lib/dateUtils'

function formatFileSize(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

interface LastScanStats {
  state: 'no_scans' | 'last_scan'
  scan_id?: number
  root_id?: number
  root_path?: string
  scan_state?: string
  scan_time?: number
  total_files?: number
  total_folders?: number
  total_file_size?: number
  total_adds?: number
  total_modifies?: number
  total_deletes?: number
  files_added?: number
  files_modified?: number
  files_deleted?: number
  folders_added?: number
  folders_modified?: number
  folders_deleted?: number
  items_hashed?: number
  items_validated?: number
  alerts_generated?: number
  hash_enabled?: boolean
  validation_enabled?: boolean
  error?: string | null
}

export function LastScanCard() {
  const [stats, setStats] = useState<LastScanStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)
  const navigate = useNavigate()
  const { isScanning, currentScanId } = useScanManager()

  const loadStats = async () => {
    try {
      setLoading(true)
      setError(null)
      const response = await fetch('/api/home/last-scan-stats')
      if (!response.ok) {
        throw new Error('Failed to fetch scan stats')
      }
      const data = await response.json()
      setStats(data)
    } catch (err) {
      console.error('Error loading scan stats:', err)
      setError(err instanceof Error ? err.message : 'Failed to load scan information')
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadStats()
  }, [])

  // Reload stats when scan completes
  useEffect(() => {
    if (!isScanning && currentScanId === null) {
      // Scan just completed, wait a moment for database to settle
      const timer = setTimeout(() => {
        loadStats()
      }, 500)
      return () => clearTimeout(timer)
    }
  }, [isScanning, currentScanId])

  if (loading) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Loading...</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground">Loading scan information...</p>
        </CardContent>
      </Card>
    )
  }

  if (error) {
    return (
      <Card>
        <CardHeader>
          <CardTitle>Error</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-red-600 dark:text-red-400">Failed to load scan information.</p>
        </CardContent>
      </Card>
    )
  }

  if (!stats || stats.state === 'no_scans') {
    return (
      <Card>
        <CardHeader>
          <CardTitle>No Scans</CardTitle>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground">
            No scans have been run yet. Go to the Scan page to start scanning your filesystem roots.
          </p>
        </CardContent>
      </Card>
    )
  }

  // If currently scanning, show "Current Scan" instead
  if (isScanning && currentScanId) {
    return (
      <Card>
        <CardHeader className="flex flex-row items-center justify-between">
          <CardTitle>Current Scan</CardTitle>
          <button
            onClick={() => navigate('/scan')}
            className="text-sm text-primary hover:opacity-70 transition-opacity font-medium"
          >
            View Details →
          </button>
        </CardHeader>
        <CardContent>
          <p className="text-muted-foreground">
            A scan is currently in progress. View the Scan page for real-time updates.
          </p>
        </CardContent>
      </Card>
    )
  }

  const isError = stats.scan_state === 'Error'
  const isStopped = stats.scan_state === 'Stopped'
  const isIncomplete = stats.scan_state === 'Scanning' // Not actively running but incomplete
  const isCompleted = !isError && !isStopped && !isIncomplete

  // Determine title and link destination
  let title = 'Last Scan'
  let linkDestination = '/explore'

  if (isError) {
    title = `Failed Scan - ${stats.root_path}`
    linkDestination = '/scan'
  } else if (isIncomplete) {
    title = `Incomplete Scan - ${stats.root_path}`
    linkDestination = '/scan'
  } else if (isStopped) {
    title = `Stopped Scan - ${stats.root_path}`
    linkDestination = '/scan'
  } else {
    title = `Last Scan - ${stats.root_path}`
  }

  const timeAgo = stats.scan_time ? formatTimeAgo(stats.scan_time * 1000) : ''

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle className="text-xl">{title}</CardTitle>
        <button
          onClick={() => navigate(linkDestination)}
          className="text-sm text-primary hover:opacity-70 transition-opacity font-medium"
        >
          View Details →
        </button>
      </CardHeader>
      <CardContent className="space-y-4">
        {/* Basic Info */}
        <div className="flex flex-col gap-1 p-4 bg-muted/40 rounded-lg text-sm">
          <div>
            <span className="text-muted-foreground">Scan ID:</span> {stats.scan_id}
          </div>
          <div>
            <span className="text-muted-foreground">Started:</span> {timeAgo}
          </div>
          {!isError && !isStopped && stats.total_files !== undefined && (
            <>
              <div>
                <span className="text-muted-foreground">Items:</span>{' '}
                {stats.total_files.toLocaleString()} files, {stats.total_folders?.toLocaleString() || 0} folders
              </div>
              {stats.total_file_size !== undefined && stats.total_file_size > 0 && (
                <div>
                  <span className="text-muted-foreground">Total Size:</span>{' '}
                  {formatFileSize(stats.total_file_size)}
                </div>
              )}
            </>
          )}
        </div>

        {/* Error Message */}
        {isError && stats.error && (
          <div className="p-3 bg-red-500/10 border border-red-500/20 rounded-md">
            <p className="text-red-600 dark:text-red-400 font-mono text-sm">
              Error: {stats.error}
            </p>
          </div>
        )}

        {/* Stopped Message */}
        {isStopped && (
          <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-md">
            <p className="text-amber-600 dark:text-amber-400 font-semibold text-sm">
              Scan was stopped - no changes recorded
            </p>
          </div>
        )}

        {/* Incomplete Message */}
        {isIncomplete && (
          <div className="p-3 bg-amber-500/10 border border-amber-500/20 rounded-md">
            <p className="text-amber-600 dark:text-amber-400 font-semibold text-sm">
              Scan incomplete - resume or cancel on the Scan page
            </p>
          </div>
        )}

        {/* Alerts Display (Completed scans only) */}
        {isCompleted && stats.alerts_generated !== undefined && (
          <div
            className={`p-3 rounded-md font-semibold text-sm ${
              stats.alerts_generated > 0
                ? 'bg-red-500/10 border border-red-500/20 text-red-600 dark:text-red-400'
                : 'bg-muted/40 text-muted-foreground'
            }`}
          >
            {stats.alerts_generated > 0
              ? `${stats.alerts_generated.toLocaleString()} ${stats.alerts_generated === 1 ? 'Alert Created' : 'Alerts Created'}`
              : 'No alerts created'}
          </div>
        )}

        {/* Changes Table (Not shown for error or stopped) */}
        {!isError && !isStopped && (
          <div className="border border-border rounded-lg overflow-hidden">
            <table className="w-full text-sm">
              <thead className="bg-muted/40">
                <tr>
                  <th className="px-4 py-3 text-left font-semibold text-xs uppercase tracking-wider text-muted-foreground">
                  </th>
                  <th className="px-4 py-3 text-center font-semibold text-xs uppercase tracking-wider text-muted-foreground">
                    Files
                  </th>
                  <th className="px-4 py-3 text-center font-semibold text-xs uppercase tracking-wider text-muted-foreground">
                    Folders
                  </th>
                  <th className="px-4 py-3 text-center font-semibold text-xs uppercase tracking-wider text-muted-foreground">
                    Total
                  </th>
                </tr>
              </thead>
              <tbody>
                <tr className="border-t border-border hover:bg-muted/20 transition-colors">
                  <td className="px-4 py-2.5 text-muted-foreground font-medium">Added</td>
                  <td className="px-4 py-2.5 text-center">{stats.files_added?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center">{stats.folders_added?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center font-semibold">{stats.total_adds?.toLocaleString() || 0}</td>
                </tr>
                <tr className="border-t border-border hover:bg-muted/20 transition-colors">
                  <td className="px-4 py-2.5 text-muted-foreground font-medium">Modified</td>
                  <td className="px-4 py-2.5 text-center">{stats.files_modified?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center">{stats.folders_modified?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center font-semibold">{stats.total_modifies?.toLocaleString() || 0}</td>
                </tr>
                <tr className="border-t border-border hover:bg-muted/20 transition-colors">
                  <td className="px-4 py-2.5 text-muted-foreground font-medium">Deleted</td>
                  <td className="px-4 py-2.5 text-center">{stats.files_deleted?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center">{stats.folders_deleted?.toLocaleString() || 0}</td>
                  <td className="px-4 py-2.5 text-center font-semibold">{stats.total_deletes?.toLocaleString() || 0}</td>
                </tr>
              </tbody>
            </table>
          </div>
        )}

        {/* Analysis Summary */}
        {!isError && !isStopped && (
          <div className="text-sm text-muted-foreground p-3 bg-muted/40 rounded-lg">
            {(() => {
              const items: string[] = []
              if (stats.hash_enabled && stats.items_hashed !== undefined) {
                items.push(`${stats.items_hashed.toLocaleString()} hashed`)
              }
              if (stats.validation_enabled && stats.items_validated !== undefined) {
                items.push(`${stats.items_validated.toLocaleString()} validated`)
              }
              if (isIncomplete && stats.alerts_generated && stats.alerts_generated > 0) {
                items.push(`${stats.alerts_generated.toLocaleString()} ${stats.alerts_generated === 1 ? 'alert' : 'alerts'}`)
              }

              return items.length > 0 ? items.join(' • ') : 'No analysis performed'
            })()}
          </div>
        )}
      </CardContent>
    </Card>
  )
}
