import { useState, useEffect } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { formatFileSize } from '@/lib/formatUtils'

interface AppInfo {
  name: string
  version: string
  git_commit: string
  git_commit_short: string
  git_branch: string
  build_timestamp: string
}

interface DbStats {
  path: string
  total_size: number
  wasted_size: number
}

export function SettingsPage() {
  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [dbStats, setDbStats] = useState<DbStats | null>(null)
  const [dbLoading, setDbLoading] = useState(true)
  const [dbError, setDbError] = useState<string | null>(null)
  const [compacting, setCompacting] = useState(false)
  const [compactionMessage, setCompactionMessage] = useState<string | null>(null)

  useEffect(() => {
    async function fetchAppInfo() {
      try {
        const response = await fetch('/api/app-info')
        if (!response.ok) {
          throw new Error('Failed to fetch app info')
        }
        const data = await response.json()
        setAppInfo(data)
      } catch (err) {
        console.error('Error fetching app info:', err)
        setError(err instanceof Error ? err.message : 'Unknown error')
      } finally {
        setLoading(false)
      }
    }

    fetchAppInfo()
  }, [])

  useEffect(() => {
    fetchDbStats()
  }, [])

  async function fetchDbStats(): Promise<DbStats | null> {
    try {
      setDbLoading(true)
      const response = await fetch('/api/database/stats')
      if (!response.ok) {
        throw new Error('Failed to fetch database stats')
      }
      const data = await response.json()
      setDbStats(data)
      setDbError(null)
      return data
    } catch (err) {
      console.error('Error fetching database stats:', err)
      setDbError(err instanceof Error ? err.message : 'Unknown error')
      return null
    } finally {
      setDbLoading(false)
    }
  }

  async function handleCompact() {
    if (!dbStats) return

    const sizeBefore = dbStats.total_size

    try {
      setCompacting(true)
      setCompactionMessage(null)

      const response = await fetch('/api/database/compact', {
        method: 'POST',
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Compaction failed')
      }

      // Fetch stats again to show savings
      const statsAfter = await fetchDbStats()

      // Calculate and show savings
      const sizeAfter = statsAfter?.total_size || 0
      const saved = sizeBefore - sizeAfter
      setCompactionMessage(`Compaction complete! Saved ${formatFileSize(saved)}`)
    } catch (err) {
      console.error('Error compacting database:', err)
      setCompactionMessage(
        err instanceof Error ? `Error: ${err.message}` : 'Compaction failed'
      )
    } finally {
      setCompacting(false)
    }
  }

  const formatTimestamp = (timestamp: string) => {
    try {
      const date = new Date(timestamp)
      return date.toLocaleString('en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
        timeZoneName: 'short'
      })
    } catch {
      return timestamp
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Database</CardTitle>
        </CardHeader>
        <CardContent>
          {dbLoading && (
            <p className="text-sm text-muted-foreground">Loading database information...</p>
          )}

          {dbError && (
            <p className="text-sm text-red-600 dark:text-red-400">Error: {dbError}</p>
          )}

          {dbStats && (
            <div className="space-y-4">
              <div className="grid grid-cols-[140px_1fr] gap-2 text-sm">
                <span className="font-medium text-muted-foreground">Path:</span>
                <span className="font-mono text-xs break-all">{dbStats.path}</span>

                <span className="font-medium text-muted-foreground">Size:</span>
                <span className="font-mono">{formatFileSize(dbStats.total_size)}</span>

                <span className="font-medium text-muted-foreground">Wasted Space:</span>
                <span className="font-mono">{formatFileSize(dbStats.wasted_size)}</span>
              </div>

              <div className="pt-3 border-t border-border space-y-3">
                <div className="flex items-center gap-3">
                  <Button
                    onClick={handleCompact}
                    disabled={compacting || dbStats.wasted_size === 0}
                    size="default"
                  >
                    {compacting ? 'Compacting...' : 'Compact Database'}
                  </Button>
                  {dbStats.wasted_size === 0 && (
                    <span className="text-xs text-muted-foreground">
                      No wasted space to reclaim
                    </span>
                  )}
                </div>

                {compactionMessage && (
                  <p
                    className={`text-sm ${
                      compactionMessage.startsWith('Error')
                        ? 'text-red-600 dark:text-red-400'
                        : 'text-green-600 dark:text-green-400'
                    }`}
                  >
                    {compactionMessage}
                  </p>
                )}

                <p className="text-xs text-muted-foreground">
                  Compacting the database reclaims wasted space from deleted data and migrations.
                  This operation may take several minutes for large databases and cannot run while
                  a scan is in progress.
                </p>
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>About</CardTitle>
        </CardHeader>
        <CardContent>
          {loading && (
            <p className="text-sm text-muted-foreground">Loading version information...</p>
          )}

          {error && (
            <p className="text-sm text-red-600 dark:text-red-400">Error: {error}</p>
          )}

          {appInfo && (
            <div className="space-y-3">
              <div className="grid grid-cols-[140px_1fr] gap-2 text-sm">
                <span className="font-medium text-muted-foreground">Version:</span>
                <span className="font-mono">{appInfo.version}</span>

                <span className="font-medium text-muted-foreground">Build Date:</span>
                <span className="font-mono text-xs">{formatTimestamp(appInfo.build_timestamp)}</span>

                <span className="font-medium text-muted-foreground">Git Branch:</span>
                <span className="font-mono">{appInfo.git_branch}</span>

                <span className="font-medium text-muted-foreground">Git Commit:</span>
                <div className="flex items-center gap-2">
                  <span className="font-mono text-xs">{appInfo.git_commit_short}</span>
                  {appInfo.git_commit !== 'unknown' && (
                    <span className="text-xs text-muted-foreground">({appInfo.git_commit})</span>
                  )}
                </div>
              </div>

              <div className="pt-3 border-t border-border">
                <p className="text-xs text-muted-foreground">
                  FsPulse is a fast, cross-platform filesystem scanner and change tracker.
                </p>
                <div className="flex flex-wrap gap-4 mt-2">
                  <a
                    href="https://github.com/gtunes-dev/fspulse"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-primary hover:underline"
                  >
                    GitHub
                  </a>
                  <a
                    href="https://gtunes-dev.github.io/fspulse/"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-primary hover:underline"
                  >
                    Documentation
                  </a>
                  <a
                    href="https://crates.io/crates/fspulse"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-primary hover:underline"
                  >
                    crates.io
                  </a>
                  <a
                    href="https://hub.docker.com/r/gtunesdev/fspulse"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="text-xs text-primary hover:underline"
                  >
                    Docker Hub
                  </a>
                </div>
              </div>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  )
}
