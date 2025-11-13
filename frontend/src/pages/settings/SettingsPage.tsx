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

interface ConfigSetting<T> {
  config_value: T
  effective_value: T
  source: string
  env_var: string
  editable: boolean
}

interface AnalysisSettings {
  threads: ConfigSetting<number>
}

interface SettingsResponse {
  analysis: AnalysisSettings
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

  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [settingsLoading, setSettingsLoading] = useState(true)
  const [settingsError, setSettingsError] = useState<string | null>(null)
  const [threadsInput, setThreadsInput] = useState<string>('')
  const [saving, setSaving] = useState(false)
  const [saveMessage, setSaveMessage] = useState<string | null>(null)

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

  useEffect(() => {
    fetchSettings()
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

  async function fetchSettings() {
    try {
      setSettingsLoading(true)
      const response = await fetch('/api/settings')
      if (!response.ok) {
        throw new Error('Failed to fetch settings')
      }
      const data = await response.json()
      setSettings(data)
      setThreadsInput(data.analysis.threads.config_value.toString())
      setSettingsError(null)
    } catch (err) {
      console.error('Error fetching settings:', err)
      setSettingsError(err instanceof Error ? err.message : 'Unknown error')
    } finally {
      setSettingsLoading(false)
    }
  }

  async function handleSaveThreads() {
    if (!settings) return

    const threads = parseInt(threadsInput, 10)

    // Validate input
    if (isNaN(threads) || threads < 1 || threads > 24) {
      setSaveMessage('Error: Threads must be a number between 1 and 24')
      return
    }

    try {
      setSaving(true)
      setSaveMessage(null)

      const response = await fetch('/api/settings', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          analysis: {
            threads: threads,
          },
        }),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Failed to save settings')
      }

      const message = await response.text()
      setSaveMessage(message)

      // Refresh settings to show updated values
      await fetchSettings()
    } catch (err) {
      console.error('Error saving settings:', err)
      setSaveMessage(
        err instanceof Error ? `Error: ${err.message}` : 'Failed to save settings'
      )
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Settings</h1>

      <Card>
        <CardHeader>
          <CardTitle>Configuration</CardTitle>
        </CardHeader>
        <CardContent>
          {settingsLoading && (
            <p className="text-sm text-muted-foreground">Loading configuration...</p>
          )}

          {settingsError && (
            <p className="text-sm text-red-600 dark:text-red-400">Error: {settingsError}</p>
          )}

          {settings && (
            <div className="space-y-4">
              <div>
                <div className="flex items-center gap-3 mb-2">
                  <label htmlFor="threads-input" className="text-sm font-medium">
                    Analysis Threads
                  </label>
                  {settings.analysis.threads.source === 'environment' && (
                    <span className="text-xs px-2 py-0.5 rounded bg-blue-100 dark:bg-blue-900 text-blue-700 dark:text-blue-300">
                      Environment Override
                    </span>
                  )}
                </div>

                <div className="grid grid-cols-[140px_1fr] gap-2 text-sm mb-3">
                  <span className="font-medium text-muted-foreground">Current Value:</span>
                  <span className="font-mono">{settings.analysis.threads.effective_value}</span>

                  {settings.analysis.threads.source === 'environment' && (
                    <>
                      <span className="font-medium text-muted-foreground">Config File:</span>
                      <span className="font-mono">{settings.analysis.threads.config_value}</span>
                    </>
                  )}
                </div>

                {settings.analysis.threads.editable ? (
                  <div className="space-y-3">
                    <div className="flex items-center gap-3">
                      <input
                        id="threads-input"
                        type="number"
                        min="1"
                        max="24"
                        value={threadsInput}
                        onChange={(e) => setThreadsInput(e.target.value)}
                        disabled={saving}
                        className="w-32 px-3 py-2 text-sm border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary disabled:opacity-50"
                      />
                      <Button
                        onClick={handleSaveThreads}
                        disabled={saving || threadsInput === settings.analysis.threads.config_value.toString()}
                        size="default"
                      >
                        {saving ? 'Saving...' : 'Save'}
                      </Button>
                    </div>

                    {saveMessage && (
                      <p
                        className={`text-sm ${
                          saveMessage.startsWith('Error')
                            ? 'text-red-600 dark:text-red-400'
                            : 'text-green-600 dark:text-green-400'
                        }`}
                      >
                        {saveMessage}
                      </p>
                    )}

                    <p className="text-xs text-muted-foreground">
                      Number of worker threads used during the analysis phase of scanning for hashing and
                      validation. Valid range: 1-24. Restart required for changes to take effect.
                    </p>
                  </div>
                ) : (
                  <div className="space-y-2">
                    <div className="px-3 py-2 text-sm bg-muted rounded-md border border-border">
                      <p className="text-muted-foreground">
                        ℹ️ This setting is overridden by the environment variable{' '}
                        <code className="font-mono text-xs bg-background px-1 py-0.5 rounded">
                          {settings.analysis.threads.env_var}
                        </code>
                      </p>
                      <p className="text-muted-foreground mt-1">
                        To edit this setting, remove the environment variable and restart the application.
                      </p>
                    </div>
                  </div>
                )}
              </div>
            </div>
          )}
        </CardContent>
      </Card>

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
