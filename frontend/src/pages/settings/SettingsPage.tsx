import { useState, useEffect } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { CircleDashed, RefreshCw } from 'lucide-react'
import { formatFileSize } from '@/lib/formatUtils'
import { useTaskContext } from '@/contexts/TaskContext'

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
  env_value: T | null
  file_value: T | null
  file_value_original: T | null
  default_value: T
  env_var: string
  requires_restart: boolean
  editable: boolean
}

interface SettingsResponse {
  analysis_threads: ConfigSetting<number>
  logging_fspulse: ConfigSetting<string>
  logging_lopdf: ConfigSetting<string>
  server_host: ConfigSetting<string>
  server_port: ConfigSetting<number>
  database_dir: ConfigSetting<string>
}

export function SettingsPage() {
  const { isExclusive, lastTaskCompletedAt } = useTaskContext()

  const [appInfo, setAppInfo] = useState<AppInfo | null>(null)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState<string | null>(null)

  const [dbStats, setDbStats] = useState<DbStats | null>(null)
  const [dbLoading, setDbLoading] = useState(true)
  const [dbError, setDbError] = useState<string | null>(null)
  const [compactionMessage, setCompactionMessage] = useState<string | null>(null)

  const [settings, setSettings] = useState<SettingsResponse | null>(null)
  const [settingsLoading, setSettingsLoading] = useState(true)
  const [settingsError, setSettingsError] = useState<string | null>(null)
  const [editingSetting, setEditingSetting] = useState<string | null>(null)
  const [editValue, setEditValue] = useState<string>('')
  const [saving, setSaving] = useState(false)
  const [saveMessage, setSaveMessage] = useState<string | null>(null)
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false)
  const [deleting, setDeleting] = useState(false)

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

  // Refresh DB stats when a task completes (picks up new sizes after VACUUM)
  useEffect(() => {
    if (lastTaskCompletedAt) {
      fetchDbStats()
    }
  }, [lastTaskCompletedAt])

  async function handleCompact() {
    try {
      setCompactionMessage(null)

      const response = await fetch('/api/tasks/compact-database', {
        method: 'POST',
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Failed to schedule compaction')
      }

      setCompactionMessage('Compaction scheduled')
    } catch (err) {
      console.error('Error scheduling database compaction:', err)
      setCompactionMessage(
        err instanceof Error ? `Error: ${err.message}` : 'Failed to schedule compaction'
      )
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
      setSettingsError(null)
    } catch (err) {
      console.error('Error fetching settings:', err)
      setSettingsError(err instanceof Error ? err.message : 'Unknown error')
    } finally {
      setSettingsLoading(false)
    }
  }

  async function handleSave() {
    if (!settings || !editingSetting) return

    try {
      setSaving(true)
      setSaveMessage(null)

      // Build request based on which setting is being edited
      let requestBody: Record<string, string | number> = {}

      if (editingSetting === 'analysis_threads') {
        const threads = parseInt(editValue, 10)
        if (isNaN(threads) || threads < 1 || threads > 24) {
          setSaveMessage('Error: Threads must be a number between 1 and 24')
          return
        }
        requestBody = { analysis_threads: threads }
      } else if (editingSetting === 'server_host') {
        requestBody = { server_host: editValue }
      } else if (editingSetting === 'server_port') {
        const port = parseInt(editValue, 10)
        if (isNaN(port) || port < 1 || port > 65535) {
          setSaveMessage('Error: Port must be a number between 1 and 65535')
          return
        }
        requestBody = { server_port: port }
      } else if (editingSetting === 'logging_fspulse') {
        requestBody = { logging_fspulse: editValue }
      } else if (editingSetting === 'logging_lopdf') {
        requestBody = { logging_lopdf: editValue }
      } else if (editingSetting === 'database_dir') {
        requestBody = { database_dir: editValue }
      }

      const response = await fetch('/api/settings', {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(requestBody),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Failed to save settings')
      }

      const message = await response.text()
      setSaveMessage(message)

      // Refresh settings to show updated values
      await fetchSettings()

      // Close modal after successful save
      setTimeout(() => {
        setEditingSetting(null)
        setSaveMessage(null)
      }, 2000)
    } catch (err) {
      console.error('Error saving settings:', err)
      setSaveMessage(
        err instanceof Error ? `Error: ${err.message}` : 'Failed to save settings'
      )
    } finally {
      setSaving(false)
    }
  }

  function handleEditSetting(settingKey: string, currentValue: string | number) {
    setEditingSetting(settingKey)
    setEditValue(String(currentValue))
    setSaveMessage(null)
    setShowDeleteConfirm(false)
  }

  async function handleDelete() {
    if (!editingSetting) return

    try {
      setDeleting(true)

      const response = await fetch('/api/settings', {
        method: 'DELETE',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ setting_key: editingSetting }),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Failed to delete setting')
      }

      // Refresh settings to show updated values
      await fetchSettings()

      // Close both dialogs after successful delete
      setEditingSetting(null)
      setShowDeleteConfirm(false)
      setSaveMessage(null)
    } catch (err) {
      console.error('Error deleting setting:', err)
      setSaveMessage(
        err instanceof Error ? `Error: ${err.message}` : 'Failed to delete setting'
      )
      // Keep confirmation dialog open to show the error
    } finally {
      setDeleting(false)
    }
  }

  const ValueDisplay = ({ value }: { value: string | number | null }) => {
    if (value === null || value === undefined || value === '') {
      return <CircleDashed className="w-4 h-4 text-muted-foreground" />
    }
    return <span className="font-mono text-sm">{String(value)}</span>
  }

  const SettingRow = ({
    name,
    description,
    setting,
    defaultValue,
    settingKey,
  }: {
    name: string
    description: string
    setting: ConfigSetting<string | number>
    defaultValue: string | number
    settingKey: string
  }) => {
    // Define which field is currently active (exactly one will be active)
    type ActiveField = 'environment' | 'file_value' | 'file_value_original' | 'default'

    // Helper to check if a value is set (non-null and non-empty)
    const isSet = (value: unknown): boolean => value !== null && value !== ''

    // Determine which field is active based on precedence (Environment > File Original > File > Default)
    let activeField: ActiveField

    if (isSet(setting.env_value)) {
      activeField = 'environment'
    } else if (isSet(setting.file_value_original) && setting.file_value_original !== setting.file_value) {
      activeField = 'file_value_original'
    } else if (isSet(setting.file_value) && setting.file_value === setting.file_value_original) {
      activeField = 'file_value'
    } else {
      activeField = 'default'
    }

    return (
      <tr className="hover:bg-muted/20">
        {/* Setting Name */}
        <td className="px-4 py-4 border-r border-border">
          <div>
            <div className="font-medium">{name}</div>
            <div className="text-xs text-muted-foreground mt-0.5">
              {description}
            </div>
          </div>
        </td>

        {/* Default Value */}
        <td className="px-4 py-4 border-r border-border">
          <div className="flex justify-center">
            <div className={`inline-flex items-center justify-center px-3 py-1.5 rounded-full ${
              activeField === 'default' && isSet(defaultValue)
                ? 'bg-card text-card-foreground border border-emerald-500'
                : ''
            }`}>
              <ValueDisplay value={defaultValue} />
            </div>
          </div>
        </td>

        {/* Config File Value + Edit Button */}
        <td className="px-2 py-4 border-r border-border">
          <div className="flex flex-col items-center gap-1">
            {setting.file_value === setting.file_value_original ? (
              // Single line case
              <div className="flex items-center justify-center gap-2">
                {setting.file_value !== null ? (
                  <div className={`inline-flex items-center justify-center px-3 py-1.5 rounded-full ${
                    activeField === 'file_value'
                      ? 'bg-card text-card-foreground border border-emerald-500'
                      : ''
                  }`}>
                    <ValueDisplay value={setting.file_value} />
                  </div>
                ) : (
                  <div className="inline-flex items-center justify-center px-3 py-1.5 rounded-full">
                    <CircleDashed className="w-4 h-4 text-muted-foreground" />
                  </div>
                )}
                <Button
                  size="sm"
                  onClick={() => handleEditSetting(settingKey, setting.file_value ?? '')}
                  className="h-7 px-2 text-xs"
                >
                  Edit
                </Button>
              </div>
            ) : (
              // Two line case (pending restart)
              <>
                <div className="flex items-center justify-center gap-2">
                  <div className="inline-flex items-center justify-center px-3 py-1.5 rounded-full gap-1.5 bg-card text-card-foreground border border-blue-500">
                    <RefreshCw className="w-3.5 h-3.5 text-blue-600 dark:text-blue-400" />
                    {setting.file_value !== null ? (
                      <span className="font-mono text-sm">{String(setting.file_value)}</span>
                    ) : (
                      <CircleDashed className="w-4 h-4 text-muted-foreground" />
                    )}
                  </div>
                  <Button
                    size="sm"
                    onClick={() => handleEditSetting(settingKey, setting.file_value ?? '')}
                    className="h-7 px-2 text-xs"
                  >
                    Edit
                  </Button>
                </div>
                {setting.file_value_original !== null && (
                  <div className="flex items-center gap-1.5 text-xs">
                    <span className="text-muted-foreground">Current:</span>
                    <div className={`inline-flex items-center justify-center px-2 py-0.5 rounded-full ${
                      activeField === 'file_value_original'
                        ? 'bg-card text-card-foreground border border-emerald-500'
                        : ''
                    }`}>
                      <span className="font-mono text-xs">{String(setting.file_value_original)}</span>
                    </div>
                  </div>
                )}
              </>
            )}
          </div>
        </td>

        {/* Environment Value */}
        <td className="px-4 py-4">
          <div className="flex justify-center">
            {setting.env_value !== null ? (
              <div className={`inline-flex items-center justify-center px-3 py-1.5 rounded-full ${
                activeField === 'environment'
                  ? 'bg-card text-card-foreground border border-emerald-500'
                  : ''
              }`}>
                <span className="font-mono text-sm font-medium">{String(setting.env_value)}</span>
              </div>
            ) : (
              <div className="inline-flex items-center justify-center px-3 py-1.5 rounded-full">
                <CircleDashed className="w-4 h-4 text-muted-foreground" />
              </div>
            )}
          </div>
        </td>
      </tr>
    )
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
            <>
              <div className="border border-border rounded-lg overflow-hidden">
                <table className="w-full text-sm">
                  <thead className="bg-muted/50 border-b border-border">
                    <tr>
                      <th className="text-left px-4 py-3 font-medium min-w-[200px] border-r border-border">
                        <div>Setting</div>
                      </th>
                      <th className="text-center px-4 py-3 font-medium w-[140px] border-r border-border">
                        <div>Default</div>
                        <div className="text-xs font-normal text-muted-foreground mt-0.5">Lowest priority</div>
                      </th>
                      <th className="text-center px-2 py-3 font-medium border-r border-border">
                        <div>Config File</div>
                        <div className="text-xs font-normal text-muted-foreground mt-0.5">Overrides default</div>
                      </th>
                      <th className="text-center px-4 py-3 font-medium w-[140px]">
                        <div>Environment</div>
                        <div className="text-xs font-normal text-muted-foreground mt-0.5">Highest priority</div>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <SettingRow
                      name="Analysis Threads"
                      description="Number of worker threads for analysis phase"
                      setting={settings.analysis_threads}
                      defaultValue={8}
                      settingKey="analysis_threads"
                    />
                    <SettingRow
                      name="FsPulse Log Level"
                      description="Logging verbosity for FsPulse"
                      setting={settings.logging_fspulse}
                      defaultValue="info"
                      settingKey="logging_fspulse"
                    />
                    <SettingRow
                      name="LoPDF Log Level"
                      description="Logging verbosity for PDF library"
                      setting={settings.logging_lopdf}
                      defaultValue="error"
                      settingKey="logging_lopdf"
                    />
                    <SettingRow
                      name="Server Host"
                      description="HTTP server bind address"
                      setting={settings.server_host}
                      defaultValue="127.0.0.1"
                      settingKey="server_host"
                    />
                    <SettingRow
                      name="Server Port"
                      description="HTTP server port"
                      setting={settings.server_port}
                      defaultValue={8080}
                      settingKey="server_port"
                    />
                    <SettingRow
                      name="Database Directory"
                      description="Location of database file (empty = use data directory)"
                      setting={settings.database_dir}
                      defaultValue=""
                      settingKey="database_dir"
                    />
                  </tbody>
                </table>
              </div>

                {/* Legend */}
                <div className="mt-4 flex items-center gap-4 text-xs text-muted-foreground flex-wrap">
                  <div className="flex items-center gap-2">
                    <div className="w-4 h-4 rounded-full bg-card border border-emerald-500" />
                    <span>Active value (currently in use)</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <CircleDashed className="w-4 h-4" />
                    <span>Not configured</span>
                  </div>
                  <div className="flex items-center gap-2">
                    <RefreshCw className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                    <span>Restart required</span>
                  </div>
                </div>

                {/* Edit Modal */}
                {editingSetting && settings && (() => {
                const settingInfo = {
                  'analysis_threads': {
                    title: 'Analysis Threads',
                    description: 'Number of worker threads for analysis phase',
                    setting: settings.analysis_threads,
                    defaultValue: 8,
                    inputType: 'number',
                    min: 1,
                    max: 24,
                  },
                  'logging_fspulse': {
                    title: 'FsPulse Log Level',
                    description: 'Logging verbosity for FsPulse',
                    setting: settings.logging_fspulse,
                    defaultValue: 'info',
                    inputType: 'select',
                    options: ['error', 'warn', 'info', 'debug', 'trace'],
                  },
                  'logging_lopdf': {
                    title: 'LoPDF Log Level',
                    description: 'Logging verbosity for PDF library',
                    setting: settings.logging_lopdf,
                    defaultValue: 'error',
                    inputType: 'select',
                    options: ['error', 'warn', 'info', 'debug', 'trace'],
                  },
                  'server_host': {
                    title: 'Server Host',
                    description: 'HTTP server bind address',
                    setting: settings.server_host,
                    defaultValue: '127.0.0.1',
                    inputType: 'text',
                  },
                  'server_port': {
                    title: 'Server Port',
                    description: 'HTTP server port',
                    setting: settings.server_port,
                    defaultValue: 8080,
                    inputType: 'number',
                    min: 1,
                    max: 65535,
                  },
                  'database_dir': {
                    title: 'Database Directory',
                    description: 'Location of database file (leave empty to use data directory)',
                    setting: settings.database_dir,
                    defaultValue: '',
                    inputType: 'text',
                  },
                }[editingSetting]

                if (!settingInfo) return null

                return (
                  <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={() => setEditingSetting(null)}>
                    <Card className="w-[500px]" onClick={(e) => e.stopPropagation()}>
                      <CardHeader>
                        <CardTitle>Edit {settingInfo.title}</CardTitle>
                      </CardHeader>
                      <CardContent className="space-y-4">
                        <div>
                          <p className="text-sm text-muted-foreground mb-3">
                            {settingInfo.description}
                          </p>

                          {/* Will take effect message */}
                          <div className="mb-4 p-3 bg-muted/50 rounded-lg">
                            <p className="text-sm">
                              {settingInfo.setting.env_value !== null ? (
                                <>
                                  <strong>Will not take effect:</strong> This setting is overridden by the{' '}
                                  <code className="font-mono bg-muted px-1 rounded">{settingInfo.setting.env_var}</code>{' '}
                                  environment variable.
                                </>
                              ) : (
                                <>
                                  <strong>Will take effect:</strong>{' '}
                                  {settingInfo.setting.requires_restart ? 'On restart.' : 'On next scan.'}
                                </>
                              )}
                            </p>
                          </div>

                          <label className="block text-sm font-medium mb-2">
                            Value for config.toml:
                          </label>

                          {settingInfo.inputType === 'number' && (
                            <input
                              type="number"
                              min={settingInfo.min}
                              max={settingInfo.max}
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              className="w-full px-3 py-2 text-sm border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
                            />
                          )}

                          {settingInfo.inputType === 'select' && (
                            <select
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              className="w-full px-3 py-2 text-sm border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
                            >
                              {settingInfo.options?.map(opt => (
                                <option key={opt} value={opt}>{opt}</option>
                              ))}
                            </select>
                          )}

                          {settingInfo.inputType === 'text' && (
                            <input
                              type="text"
                              value={editValue}
                              onChange={(e) => setEditValue(e.target.value)}
                              className="w-full px-3 py-2 text-sm border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-primary"
                            />
                          )}

                          {settingInfo.inputType === 'number' && settingInfo.min !== undefined && settingInfo.max !== undefined && (
                            <p className="text-xs text-muted-foreground mt-1">
                              Valid range: {settingInfo.min}–{settingInfo.max}
                            </p>
                          )}
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

                        <div className="flex justify-between pt-4">
                          <Button
                            variant="outline"
                            onClick={() => setShowDeleteConfirm(true)}
                            className="text-red-600 dark:text-red-400 hover:bg-red-50 dark:hover:bg-red-950"
                          >
                            Delete
                          </Button>
                          <div className="flex gap-2">
                            <Button variant="outline" onClick={() => setEditingSetting(null)}>
                              Cancel
                            </Button>
                            <Button onClick={handleSave} disabled={saving}>
                              {saving ? 'Saving...' : 'Save to Config File'}
                            </Button>
                          </div>
                        </div>
                      </CardContent>
                    </Card>
                  </div>
                )
              })()}

              {/* Delete Confirmation Dialog (nested on top of edit dialog) */}
              {showDeleteConfirm && (
                <div
                  className="fixed inset-0 bg-black/50 flex items-center justify-center"
                  style={{ zIndex: 60 }}
                  onClick={() => {
                    setShowDeleteConfirm(false)
                    setSaveMessage(null)
                  }}
                >
                  <Card className="w-[400px]" onClick={(e) => e.stopPropagation()}>
                    <CardHeader>
                      <CardTitle>Delete Setting from Config File?</CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-4">
                      <p className="text-sm text-muted-foreground">
                        The next time this setting is evaluated, it will use the environment variable (if set) or the default value.
                      </p>

                      {saveMessage && (
                        <p className="text-sm text-red-600 dark:text-red-400">
                          {saveMessage}
                        </p>
                      )}

                      <div className="flex justify-end gap-2 pt-2">
                        <Button
                          variant="outline"
                          onClick={() => {
                            setShowDeleteConfirm(false)
                            setSaveMessage(null)
                          }}
                          disabled={deleting}
                        >
                          Cancel
                        </Button>
                        <Button
                          onClick={handleDelete}
                          disabled={deleting}
                          className="bg-red-600 hover:bg-red-700 text-white"
                        >
                          {deleting ? 'Deleting...' : 'Delete'}
                        </Button>
                      </div>
                    </CardContent>
                  </Card>
                </div>
              )}
            </>
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
                    disabled={isExclusive || dbStats.wasted_size === 0}
                    size="default"
                  >
                    Compact Database
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
                  This operation runs as a background task and may take several minutes for large databases.
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
