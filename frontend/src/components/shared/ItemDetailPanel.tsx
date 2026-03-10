import { useState, useEffect, useCallback } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, FileX, FolderX, Calendar as CalendarIcon,
  HardDrive, AlertTriangle, CircleX, ChevronDown, Eye, X,
} from 'lucide-react'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '@/components/ui/chart'
import {
  LineChart,
  Line,
  XAxis,
  YAxis,
  CartesianGrid,
} from 'recharts'
import { fetchQuery, countQuery, updateAlertStatus } from '@/lib/api'
import type { ColumnSpec, AlertStatusValue } from '@/lib/types'
import { formatDateFull, formatDateTimeShort, formatScanDate } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'
import { cn } from '@/lib/utils'

// ---- Types ----

interface ItemDetailPanelProps {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
  scanId: number
  onClose: () => void
}

interface VersionEntry {
  version_id: number
  first_scan_id: number
  last_scan_id: number
  first_scan_date: number
  last_scan_date: number
  is_deleted: boolean
  access: number
  mod_date: number | null
  size: number | null
  add_count: number | null
  modify_count: number | null
  delete_count: number | null
  unchanged_count: number | null
  hash_state: number | null
  file_hash: string | null
  val_state: number | null
  val_error: string | null
}

interface VersionHistoryInitResponse {
  versions: VersionEntry[]
  anchor_index: number | null
  has_more: boolean
  total_count: number
  first_seen_scan_id: number
  first_seen_scan_date: number
  anchor_scan_date: number
}

interface VersionHistoryPageResponse {
  versions: VersionEntry[]
  has_more: boolean
}

interface Alert {
  alert_id: number
  scan_id: number
  alert_type: string
  alert_status: string
  val_error: string | null
  created: number
}

interface SizeHistoryPoint {
  scan_id: number
  started_at: number
  size: number
}

interface ChildrenCounts {
  file_count: number
  directory_count: number
}

interface IntegrityState {
  has_validator: boolean
  hash_state: number | null
  file_hash: string | null
  val_state: number | null
  val_error: string | null
}

type ChangeKind = 'initial' | 'modified' | 'deleted' | 'restored'

interface VersionChange {
  version: VersionEntry
  kind: ChangeKind
  prev: VersionEntry | null
}

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y'

// ---- Constants ----

const VERSIONS_PER_PAGE = 100
const ALERTS_PER_PAGE = 20

const ALERT_COLUMNS: ColumnSpec[] = [
  { name: 'alert_id', visible: true, sort_direction: 'desc', position: 0 },
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 1 },
  { name: 'alert_type', visible: true, sort_direction: 'none', position: 2 },
  { name: 'alert_status', visible: true, sort_direction: 'none', position: 3 },
  { name: 'val_error', visible: true, sort_direction: 'none', position: 4 },
  { name: 'created_at', visible: true, sort_direction: 'none', position: 5 },
]

// ---- Helpers ----

function parseAlertRow(row: string[]): Alert {
  return {
    alert_id: parseInt(row[0]),
    scan_id: parseInt(row[1]),
    alert_type: row[2],
    alert_status: row[3],
    val_error: row[4] && row[4] !== '-' ? row[4] : null,
    created: parseInt(row[5]),
  }
}

function accessLabel(access: number): string {
  switch (access) {
    case 0: return 'No Error'
    case 1: return 'Meta Error'
    case 2: return 'Read Error'
    default: return `Unknown (${access})`
  }
}

function alertTypeLabel(type: string): string {
  switch (type) {
    case 'H': return 'Suspect Hash'
    case 'I': return 'Invalid Item'
    case 'A': return 'Access Denied'
    default: return type
  }
}

function itemTypeLabel(type: string): string {
  switch (type) {
    case 'F': return 'File'
    case 'D': return 'Directory'
    case 'S': return 'Symlink'
    default: return 'Other'
  }
}

function classifyChange(version: VersionEntry, prev: VersionEntry | null): ChangeKind {
  if (!prev) return 'initial'
  if (version.is_deleted && !prev.is_deleted) return 'deleted'
  if (!version.is_deleted && prev.is_deleted) return 'restored'
  return 'modified'
}

function buildChanges(versions: VersionEntry[]): VersionChange[] {
  if (versions.length === 0) return []
  const changes: VersionChange[] = []
  for (let i = 0; i < versions.length; i++) {
    const version = versions[i]
    const prev = i + 1 < versions.length ? versions[i + 1] : null
    changes.push({ version, kind: classifyChange(version, prev), prev })
  }
  return changes
}

function hasNonZeroFolderCounts(v: VersionEntry): boolean {
  return (
    (v.add_count ?? 0) > 0 ||
    (v.modify_count ?? 0) > 0 ||
    (v.delete_count ?? 0) > 0 ||
    (v.unchanged_count ?? 0) > 0
  )
}

function hasFolderCountChanges(v: VersionEntry, prev: VersionEntry): boolean {
  return (
    v.add_count !== prev.add_count ||
    v.modify_count !== prev.modify_count ||
    v.delete_count !== prev.delete_count ||
    v.unchanged_count !== prev.unchanged_count
  )
}

function hasFieldChanges(v: VersionEntry, prev: VersionEntry): boolean {
  return (
    v.mod_date !== prev.mod_date ||
    v.size !== prev.size ||
    v.access !== prev.access ||
    hasFolderCountChanges(v, prev)
  )
}

// ---- Component ----

export function ItemDetailPanel({
  itemId,
  itemPath,
  itemType,
  isTombstone,
  scanId,
  onClose,
}: ItemDetailPanelProps) {
  // Version history state
  const [versions, setVersions] = useState<VersionEntry[]>([])
  const [loadingVersions, setLoadingVersions] = useState(false)
  const [loadingMoreVersions, setLoadingMoreVersions] = useState(false)
  const [hasMoreVersions, setHasMoreVersions] = useState(false)
  const [totalVersionCount, setTotalVersionCount] = useState(0)
  const [anchorScanDate, setAnchorScanDate] = useState(0)
  const [openVersions, setOpenVersions] = useState<Record<number, boolean>>({})

  // Alerts state
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [totalAlerts, setTotalAlerts] = useState(0)
  const [loadingMoreAlerts, setLoadingMoreAlerts] = useState(false)
  const [updatingAlertId, setUpdatingAlertId] = useState<number | null>(null)

  // Size history state
  const [sizeHistory, setSizeHistory] = useState<SizeHistoryPoint[]>([])
  const [timeWindow, setTimeWindow] = useState<TimeWindowPreset>('3m')
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [loadingSizeHistory, setLoadingSizeHistory] = useState(false)

  // Children counts state
  const [childrenCounts, setChildrenCounts] = useState<ChildrenCounts | null>(null)
  const [loadingChildrenCounts, setLoadingChildrenCounts] = useState(false)

  // Integrity state (files only)
  const [integrityState, setIntegrityState] = useState<IntegrityState | null>(null)
  const [hashExpanded, setHashExpanded] = useState(false)
  const [pathExpanded, setPathExpanded] = useState(false)

  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath
  const anchorVersion = versions.length > 0 ? versions[0] : null
  const changes = buildChanges(versions)

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
    setPathExpanded(false)
  }, [itemId])

  // ---- Data loading ----

  useEffect(() => {
    async function loadData() {
      setLoadingVersions(true)
      try {
        const versionResponse = await fetch(
          `/api/items/${itemId}/version-history?scan_id=${scanId}&limit=${VERSIONS_PER_PAGE}`
        )
        if (versionResponse.ok) {
          const data: VersionHistoryInitResponse = await versionResponse.json()
          setVersions(data.versions)
          setHasMoreVersions(data.has_more)
          setTotalVersionCount(data.total_count)
          setAnchorScanDate(data.anchor_scan_date)
        }

        const alertCountResponse = await countQuery('alerts', {
          columns: [{ name: 'alert_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [{ column: 'item_id', value: itemId.toString() }],
        })
        setTotalAlerts(alertCountResponse.count)

        const alertResponse = await fetchQuery('alerts', {
          columns: ALERT_COLUMNS,
          filters: [{ column: 'item_id', value: itemId.toString() }],
          limit: ALERTS_PER_PAGE,
          offset: 0,
        })
        setAlerts(alertResponse.rows.map(parseAlertRow))
      } catch (error) {
        console.error('Error loading item details:', error)
      } finally {
        setLoadingVersions(false)
      }
    }

    loadData()
  }, [itemId, scanId])

  const loadMoreVersions = async () => {
    if (versions.length === 0) return
    const lastVersion = versions[versions.length - 1]
    setLoadingMoreVersions(true)
    try {
      const response = await fetch(
        `/api/items/${itemId}/version-history?before_scan_id=${lastVersion.first_scan_id}&limit=${VERSIONS_PER_PAGE}`
      )
      if (response.ok) {
        const data: VersionHistoryPageResponse = await response.json()
        setVersions(prev => [...prev, ...data.versions])
        setHasMoreVersions(data.has_more)
      }
    } catch (error) {
      console.error('Error loading more versions:', error)
    } finally {
      setLoadingMoreVersions(false)
    }
  }

  const loadMoreAlerts = async () => {
    setLoadingMoreAlerts(true)
    try {
      const alertResponse = await fetchQuery('alerts', {
        columns: ALERT_COLUMNS,
        filters: [{ column: 'item_id', value: itemId.toString() }],
        limit: ALERTS_PER_PAGE,
        offset: alerts.length,
      })
      setAlerts(prev => [...prev, ...alertResponse.rows.map(parseAlertRow)])
    } catch (error) {
      console.error('Error loading more alerts:', error)
    } finally {
      setLoadingMoreAlerts(false)
    }
  }

  const handleAlertStatusUpdate = useCallback(async (alertId: number, newStatus: AlertStatusValue) => {
    setUpdatingAlertId(alertId)
    try {
      await updateAlertStatus(alertId, { status: newStatus })
      setAlerts(prev => prev.map(a =>
        a.alert_id === alertId ? { ...a, alert_status: newStatus } : a
      ))
    } catch (error) {
      console.error('Error updating alert status:', error)
    } finally {
      setUpdatingAlertId(null)
    }
  }, [])

  // ---- Size history ----

  const getFromDateForPreset = (preset: TimeWindowPreset): Date => {
    const today = startOfDay(new Date())
    switch (preset) {
      case '7d': return subDays(today, 7)
      case '30d': return subDays(today, 30)
      case '3m': return subMonths(today, 3)
      case '6m': return subMonths(today, 6)
      case '1y': return subYears(today, 1)
    }
  }

  useEffect(() => {
    setFromDate(getFromDateForPreset(timeWindow))
  }, [timeWindow])

  useEffect(() => {
    setFromDate(subMonths(startOfDay(new Date()), 3))
  }, [])

  const loadSizeHistory = useCallback(async () => {
    if (!fromDate) return
    setLoadingSizeHistory(true)
    try {
      const fromDateStr = format(fromDate, 'yyyy-MM-dd')
      const response = await fetch(
        `/api/items/${itemId}/size-history?from_date=${fromDateStr}&to_scan_id=${scanId}`
      )
      if (response.ok) {
        const data = await response.json()
        setSizeHistory(data.history || [])
      } else {
        setSizeHistory([])
      }
    } catch (error) {
      console.error('Error loading size history:', error)
      setSizeHistory([])
    } finally {
      setLoadingSizeHistory(false)
    }
  }, [itemId, fromDate, scanId])

  useEffect(() => {
    loadSizeHistory()
  }, [loadSizeHistory])

  // ---- Children counts ----

  useEffect(() => {
    async function loadChildrenCounts() {
      if (itemType !== 'D' || isTombstone) {
        setChildrenCounts(null)
        return
      }
      setLoadingChildrenCounts(true)
      try {
        const response = await fetch(`/api/items/${itemId}/children-counts?scan_id=${scanId}`)
        if (response.ok) {
          setChildrenCounts(await response.json())
        } else {
          setChildrenCounts(null)
        }
      } catch {
        setChildrenCounts(null)
      } finally {
        setLoadingChildrenCounts(false)
      }
    }
    loadChildrenCounts()
  }, [itemId, itemType, isTombstone, scanId])

  // ---- Integrity state (files only) ----

  useEffect(() => {
    if (itemType !== 'F') {
      setIntegrityState(null)
      return
    }
    async function loadIntegrity() {
      try {
        const response = await fetch(`/api/items/${itemId}/integrity-state?scan_id=${scanId}`)
        if (response.ok) {
          setIntegrityState(await response.json())
        } else {
          setIntegrityState(null)
        }
      } catch {
        setIntegrityState(null)
      }
    }
    loadIntegrity()
  }, [itemId, itemType, scanId])

  // ---- Badge renderers ----

  const getChangeIndicator = (kind: ChangeKind) => {
    const dotColor =
      kind === 'initial' ? 'bg-green-500' :
      kind === 'modified' ? 'bg-blue-500' :
      kind === 'deleted' ? 'bg-red-500' :
      'bg-green-500' // restored
    const label =
      kind === 'initial' ? 'Added' :
      kind === 'modified' ? 'Modified' :
      kind === 'deleted' ? 'Deleted' :
      'Restored'
    return (
      <span className="inline-flex items-center gap-1.5 text-xs flex-shrink-0">
        <span className={`inline-block w-[7px] h-[7px] rounded-full ${dotColor}`} />
        <span>{label}</span>
      </span>
    )
  }





  const scanRef = (id: number, date: number) => (
    <>Scan <span className="font-mono font-semibold">#{id}</span> ({formatScanDate(date)})</>
  )

  const scanRangeLabel = (v: VersionEntry) => {
    if (v.first_scan_id === v.last_scan_id) return scanRef(v.first_scan_id, v.first_scan_date)
    return (
      <>
        #{v.first_scan_id} ({formatScanDate(v.first_scan_date)}) &ndash; #{v.last_scan_id} ({formatScanDate(v.last_scan_date)})
      </>
    )
  }

  // ---- Render ----

  return (
    <div className="flex flex-col">
      {/* Compact header */}
      <div className="bg-card px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2">
          <div className="flex-shrink-0">
            {isTombstone ? (
              itemType === 'D' ? <FolderX className="h-5 w-5 text-destructive" /> : <FileX className="h-5 w-5 text-destructive" />
            ) : (
              itemType === 'D' ? <Folder className="h-5 w-5 text-foreground" /> : <File className="h-5 w-5 text-muted-foreground" />
            )}
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-base font-semibold truncate">{itemName}</p>
          </div>
          <Button variant="ghost" size="sm" className="h-6 w-6 p-0 flex-shrink-0" onClick={onClose}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
        <button
          className="text-xs text-muted-foreground mt-0.5 pl-7 flex items-center gap-1 text-left w-full hover:text-foreground transition-colors"
          onClick={() => setPathExpanded(!pathExpanded)}
        >
          <span className={pathExpanded ? 'break-all' : 'truncate'}>{itemPath}</span>
          <ChevronDown className={cn("h-3 w-3 flex-shrink-0 transition-transform", !pathExpanded && "-rotate-90")} />
        </button>
        {isTombstone && (
          <div className="mt-1 pl-7">
            <Badge variant="destructive" className="text-xs">Deleted</Badge>
          </div>
        )}
        {anchorVersion && (
          <p className="text-xs text-muted-foreground mt-1 pl-7">
            Item <span className="font-mono font-semibold text-foreground">#{itemId}</span>
            <span className="mx-1.5">&middot;</span>
            Version <span className="font-mono font-semibold text-foreground">#{anchorVersion.version_id}</span>
          </p>
        )}
      </div>

      {loadingVersions ? (
        <div className="flex items-center justify-center h-32 text-muted-foreground text-sm">
          Loading...
        </div>
      ) : (
        <div className="divide-y divide-border">
          {/* Current State */}
          {anchorVersion && (
            <div className="px-3 py-3">
              <div className="mb-2">
                <p className="text-sm font-semibold">
                  {scanRef(scanId, anchorScanDate)}
                </p>
              </div>
              <div className="grid grid-cols-2 gap-2 text-sm pl-2">
                <div>
                  <p className="text-muted-foreground">Type</p>
                  <p className="font-medium">{itemTypeLabel(itemType)}</p>
                </div>
                <div>
                  <p className="text-muted-foreground">Modified</p>
                  <p className="font-medium">{anchorVersion.mod_date ? formatDateFull(anchorVersion.mod_date) : 'N/A'}</p>
                </div>
                {anchorVersion.size !== null && (
                  <div>
                    <p className="text-muted-foreground">Size</p>
                    <p className="font-medium">{formatFileSize(anchorVersion.size)}</p>
                  </div>
                )}
              </div>

              {itemType === 'F' && integrityState && (
                <div className="mt-2 pt-2 border-t">
                  <p className="text-sm font-semibold mb-2">Integrity</p>
                  <div className="text-sm pl-2 space-y-2">
                    <div>
                      <div className="flex items-center gap-1">
                        <span className="text-muted-foreground">Hash State :</span>
                        <span>{integrityState.hash_state === 2 ? 'Suspect' : integrityState.hash_state === 1 ? 'Valid' : 'Unknown'}</span>
                        {integrityState.hash_state === 2 && <AlertTriangle className="h-3.5 w-3.5 text-amber-500" />}
                      </div>
                      {integrityState.file_hash && (
                        <div
                          className="flex items-center gap-1 mt-0.5 cursor-pointer"
                          onClick={() => setHashExpanded(!hashExpanded)}
                        >
                          <p className="font-mono text-xs break-all">
                            {hashExpanded ? integrityState.file_hash : integrityState.file_hash.slice(0, 8) + '\u2026'}
                          </p>
                          <ChevronDown className={cn("h-3 w-3 text-muted-foreground flex-shrink-0 transition-transform", !hashExpanded && "-rotate-90")} />
                        </div>
                      )}
                    </div>
                    <div>
                      <div className="flex items-center gap-1">
                        <span className="text-muted-foreground">Validation State :</span>
                        <span>
                          {integrityState.val_state === 2 ? 'Invalid'
                            : integrityState.val_state === 1 ? 'Valid'
                            : integrityState.val_state === 3 ? 'No Validator'
                            : !integrityState.has_validator ? 'No Validator'
                            : 'Unknown'}
                        </span>
                        {integrityState.val_state === 2 && <CircleX className="h-3.5 w-3.5 text-rose-500" />}
                      </div>
                      {integrityState.val_error && integrityState.val_error.trim() !== '' && (
                        <p className="text-xs mt-0.5">{integrityState.val_error}</p>
                      )}
                    </div>
                  </div>
                </div>
              )}

              {itemType === 'D' && !isTombstone && (
                <div className="mt-2 pt-2 border-t pl-2">
                  {loadingChildrenCounts ? (
                    <p className="text-sm text-muted-foreground text-center">Loading...</p>
                  ) : childrenCounts && (childrenCounts.file_count > 0 || childrenCounts.directory_count > 0) ? (
                    <>
                      <div className="flex items-center justify-center gap-4 text-sm">
                        <span className="flex items-center gap-1">
                          <Folder className="h-3 w-3 text-muted-foreground" />
                          <span className="font-medium">{childrenCounts.directory_count.toLocaleString()}</span>
                        </span>
                        <span className="flex items-center gap-1">
                          <File className="h-3 w-3 text-muted-foreground" />
                          <span className="font-medium">{childrenCounts.file_count.toLocaleString()}</span>
                        </span>
                      </div>
                      {anchorVersion && (
                        <div className="mt-3 text-xs">
                          <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                            <span className="flex items-center gap-1.5">
                              <span className="inline-block w-[7px] h-[7px] rounded-full bg-green-500" />
                              <span className="text-muted-foreground">Added :</span>
                              <span className="font-medium">{(anchorVersion.add_count ?? 0).toLocaleString()}</span>
                            </span>
                            <span className="flex items-center gap-1.5">
                              <span className="inline-block w-[7px] h-[7px] rounded-full bg-red-500" />
                              <span className="text-muted-foreground">Deleted :</span>
                              <span className="font-medium">{(anchorVersion.delete_count ?? 0).toLocaleString()}</span>
                            </span>
                            <span className="flex items-center gap-1.5">
                              <span className="inline-block w-[7px] h-[7px] rounded-full bg-blue-500" />
                              <span className="text-muted-foreground">Modified :</span>
                              <span className="font-medium">{(anchorVersion.modify_count ?? 0).toLocaleString()}</span>
                            </span>
                            <span className="flex items-center gap-1.5">
                              <span className="inline-block w-[7px] h-[7px] rounded-full bg-zinc-400" />
                              <span className="text-muted-foreground">Unchanged :</span>
                              <span className="font-medium">{(anchorVersion.unchanged_count ?? 0).toLocaleString()}</span>
                            </span>
                          </div>
                        </div>
                      )}
                    </>
                  ) : (
                    <p className="text-sm text-muted-foreground text-center">Empty directory</p>
                  )}
                </div>
              )}
            </div>
          )}

          {/* Size History */}
          <div className="px-3 py-3">
            <div className="flex items-center justify-between mb-2">
              <p className="text-sm font-semibold">Size History</p>
              <Select value={timeWindow} onValueChange={(v) => setTimeWindow(v as TimeWindowPreset)}>
                <SelectTrigger className="h-6 w-[100px] text-xs">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="7d">7 Days</SelectItem>
                  <SelectItem value="30d">30 Days</SelectItem>
                  <SelectItem value="3m">3 Months</SelectItem>
                  <SelectItem value="6m">6 Months</SelectItem>
                  <SelectItem value="1y">1 Year</SelectItem>
                </SelectContent>
              </Select>
            </div>
            {loadingSizeHistory ? (
              <div className="flex items-center justify-center h-32 text-sm text-muted-foreground">Loading...</div>
            ) : sizeHistory.length === 0 ? (
              <div className="flex items-center justify-center h-32 text-sm text-muted-foreground">No size history</div>
            ) : (
              <ChartContainer
                config={{ size: { label: 'Size', color: 'hsl(271 81% 56%)' } }}
                className="aspect-auto h-[180px]"
              >
                <LineChart data={sizeHistory.map(p => ({ date: format(new Date(p.started_at * 1000), 'MMM dd'), size: p.size }))}>
                  <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                  <XAxis dataKey="date" tick={{ fill: 'hsl(var(--muted-foreground))', fontSize: 10 }} />
                  <YAxis
                    tick={{ fill: 'hsl(var(--muted-foreground))', fontSize: 10 }}
                    tickFormatter={(value) => {
                      const units = ['B', 'KB', 'MB', 'GB', 'TB']
                      let i = 0; let s = value as number
                      while (s >= 1024 && i < units.length - 1) { s /= 1024; i++ }
                      return `${s.toFixed(0)} ${units[i]}`
                    }}
                    width={50}
                  />
                  <ChartTooltip content={<ChartTooltipContent />} formatter={(v) => formatFileSize(v as number)} />
                  <Line type="step" dataKey="size" stroke="var(--color-size)" strokeWidth={2} dot={false} />
                </LineChart>
              </ChartContainer>
            )}
          </div>

          {/* Version History */}
          <div className="px-3 py-3">
            <div className="flex items-center justify-between mb-2">
              <p className="text-sm font-semibold">Version History</p>
              {totalVersionCount > 0 && (
                <p className="text-xs text-muted-foreground">
                  {versions.length}/{totalVersionCount}
                </p>
              )}
            </div>
            {changes.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">No version history</p>
            ) : (
              <>
                <div className="divide-y divide-border">
                  {changes.map((change) => {
                    const v = change.version
                    const isAnchor = changes[0].version.version_id === v.version_id
                    const isOpen = openVersions[v.version_id] || false
                    const setIsOpen = (open: boolean) => setOpenVersions(prev => ({ ...prev, [v.version_id]: open }))
                    const hasIntegrity = v.hash_state != null || v.val_state != null
                    const hasMetadataChanges = change.kind === 'modified' && change.prev && hasFieldChanges(v, change.prev)
                    const hasInitialFolderCounts = change.kind === 'initial' && hasNonZeroFolderCounts(v)
                    const isExpandable = hasMetadataChanges || hasInitialFolderCounts || hasIntegrity
                    const prevForCounts = change.prev ?? {
                      add_count: 0, modify_count: 0, delete_count: 0, unchanged_count: 0,
                    }

                    return (
                      <div key={v.version_id} className={cn("px-2 py-1.5", isAnchor && "bg-accent/30")}>
                        {isExpandable ? (
                          <Collapsible open={isOpen} onOpenChange={setIsOpen}>
                            <div className="flex items-center gap-1.5">
                              <CollapsibleTrigger asChild>
                                <Button variant="ghost" size="icon" className="h-4 w-4 p-0 flex-shrink-0">
                                  <ChevronDown className={cn("h-2.5 w-2.5 transition-transform", !isOpen && "-rotate-90")} />
                                </Button>
                              </CollapsibleTrigger>
                              {getChangeIndicator(change.kind)}
                              {v.hash_state === 2 && <AlertTriangle className="h-3 w-3 text-amber-500 flex-shrink-0" />}
                              {v.val_state === 2 && <CircleX className="h-3 w-3 text-rose-500 flex-shrink-0" />}
                              <p className="text-xs text-muted-foreground truncate flex-1">
                                {scanRangeLabel(v)}
                              </p>
                              {isAnchor && <Eye className="h-3 w-3 text-primary flex-shrink-0" />}
                            </div>
                            <CollapsibleContent className="mt-1 ml-5.5">
                              <div className="space-y-1 text-xs">
                                {change.prev && v.mod_date !== change.prev.mod_date && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium flex items-center gap-1"><CalendarIcon className="h-2.5 w-2.5" />Modified</p>
                                    <p className="text-muted-foreground">{change.prev.mod_date ? formatDateFull(change.prev.mod_date) : 'N/A'} &rarr; {v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}</p>
                                  </div>
                                )}
                                {change.prev && v.size !== change.prev.size && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium flex items-center gap-1"><HardDrive className="h-2.5 w-2.5" />Size</p>
                                    <p className="text-muted-foreground">{formatFileSize(change.prev.size)} &rarr; {formatFileSize(v.size)}</p>
                                  </div>
                                )}
                                {change.prev && v.access !== change.prev.access && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Access</p>
                                    <p className="text-muted-foreground">{accessLabel(change.prev.access)} &rarr; {accessLabel(v.access)}</p>
                                  </div>
                                )}
                                {v.file_hash != null ? (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium flex items-center gap-1">
                                      Hash
                                      {v.hash_state === 2 && <AlertTriangle className="h-2.5 w-2.5 text-amber-500" />}
                                    </p>
                                    <p className="font-mono break-all text-muted-foreground">{v.file_hash}</p>
                                  </div>
                                ) : v.hash_state != null && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Hash</p>
                                    <p className="text-muted-foreground">Not available</p>
                                  </div>
                                )}
                                {v.val_state != null ? (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium flex items-center gap-1">
                                      Validation
                                      {v.val_state === 2 && <CircleX className="h-2.5 w-2.5 text-rose-500" />}
                                    </p>
                                    <p className="text-muted-foreground">
                                      {v.val_state === 2 ? 'Invalid' : v.val_state === 1 ? 'Valid' : v.val_state === 3 ? 'No Validator' : 'Unknown'}
                                    </p>
                                    {v.val_error && v.val_error.trim() !== '' && (
                                      <p className="font-mono break-all text-muted-foreground mt-0.5">{v.val_error}</p>
                                    )}
                                  </div>
                                ) : v.hash_state != null && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Validation</p>
                                    <p className="text-muted-foreground">Not available</p>
                                  </div>
                                )}
                                {((v.add_count ?? 0) !== (prevForCounts.add_count ?? 0) || (v.delete_count ?? 0) !== (prevForCounts.delete_count ?? 0) || (v.modify_count ?? 0) !== (prevForCounts.modify_count ?? 0) || (v.unchanged_count ?? 0) !== (prevForCounts.unchanged_count ?? 0)) && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Folder Counts</p>
                                    <div className="mt-1 space-y-0.5">
                                      {(v.add_count ?? 0) !== (prevForCounts.add_count ?? 0) && (
                                        <div className="flex items-center gap-1.5">
                                          <span className="inline-block w-[7px] h-[7px] rounded-full bg-green-500" />
                                          <span className="text-muted-foreground">Added :</span>
                                          <span className="text-muted-foreground">{(prevForCounts.add_count ?? 0).toLocaleString()}</span>
                                          <span>&rarr;</span>
                                          <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                        </div>
                                      )}
                                      {(v.delete_count ?? 0) !== (prevForCounts.delete_count ?? 0) && (
                                        <div className="flex items-center gap-1.5">
                                          <span className="inline-block w-[7px] h-[7px] rounded-full bg-red-500" />
                                          <span className="text-muted-foreground">Deleted :</span>
                                          <span className="text-muted-foreground">{(prevForCounts.delete_count ?? 0).toLocaleString()}</span>
                                          <span>&rarr;</span>
                                          <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                        </div>
                                      )}
                                      {(v.modify_count ?? 0) !== (prevForCounts.modify_count ?? 0) && (
                                        <div className="flex items-center gap-1.5">
                                          <span className="inline-block w-[7px] h-[7px] rounded-full bg-blue-500" />
                                          <span className="text-muted-foreground">Modified :</span>
                                          <span className="text-muted-foreground">{(prevForCounts.modify_count ?? 0).toLocaleString()}</span>
                                          <span>&rarr;</span>
                                          <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                        </div>
                                      )}
                                      {(v.unchanged_count ?? 0) !== (prevForCounts.unchanged_count ?? 0) && (
                                        <div className="flex items-center gap-1.5">
                                          <span className="inline-block w-[7px] h-[7px] rounded-full bg-zinc-400" />
                                          <span className="text-muted-foreground">Unchanged :</span>
                                          <span className="text-muted-foreground">{(prevForCounts.unchanged_count ?? 0).toLocaleString()}</span>
                                          <span>&rarr;</span>
                                          <span className="font-medium">{(v.unchanged_count ?? 0).toLocaleString()}</span>
                                        </div>
                                      )}
                                    </div>
                                  </div>
                                )}
                              </div>
                            </CollapsibleContent>
                          </Collapsible>
                        ) : (
                          <div className="flex items-center gap-1.5">
                            <div className="h-4 w-4 flex-shrink-0" />
                            {getChangeIndicator(change.kind)}
                            {v.hash_state === 2 && <AlertTriangle className="h-3 w-3 text-amber-500 flex-shrink-0" />}
                            {v.val_state === 2 && <CircleX className="h-3 w-3 text-rose-500 flex-shrink-0" />}
                            <p className="text-xs text-muted-foreground truncate flex-1">
                              {scanRangeLabel(v)}
                            </p>
                            {isAnchor && <Eye className="h-3 w-3 text-primary flex-shrink-0" />}
                          </div>
                        )}
                      </div>
                    )
                  })}
                </div>
                {hasMoreVersions && (
                  <div className="mt-2 flex justify-center">
                    <Button variant="outline" size="sm" className="text-sm h-7" onClick={loadMoreVersions} disabled={loadingMoreVersions}>
                      {loadingMoreVersions ? 'Loading...' : 'Load older'}
                    </Button>
                  </div>
                )}
              </>
            )}
          </div>

          {/* Alerts */}
          <div className="px-3 py-3 border-b border-border">
            <div className="flex items-center justify-between mb-2">
              <p className="text-sm font-semibold">Alerts</p>
              {totalAlerts > 0 && (
                <p className="text-xs text-muted-foreground">{alerts.length}/{totalAlerts}</p>
              )}
            </div>
            {totalAlerts === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">No alerts</p>
            ) : (
              <>
                <div className="divide-y divide-border">
                  {alerts.map((alert) => (
                    <div key={alert.alert_id} className="px-2 py-2 space-y-1.5">
                      <p className="text-xs">
                        <span className="font-mono font-semibold">#{alert.alert_id}</span>
                        {' '}<span className="text-muted-foreground">{formatDateTimeShort(alert.created)}</span>
                      </p>
                      <div className="text-xs space-y-1 pl-1">
                        <div className="flex items-center gap-1.5">
                          <span className="text-muted-foreground">Status :</span>
                          <Select
                            value={alert.alert_status}
                            onValueChange={(value) => handleAlertStatusUpdate(alert.alert_id, value as AlertStatusValue)}
                            disabled={updatingAlertId === alert.alert_id}
                          >
                            <SelectTrigger className="h-6 w-[100px] text-xs border-dashed">
                              <SelectValue />
                            </SelectTrigger>
                            <SelectContent>
                              <SelectItem value="O">Open</SelectItem>
                              <SelectItem value="F">Flagged</SelectItem>
                              <SelectItem value="D">Dismissed</SelectItem>
                            </SelectContent>
                          </Select>
                        </div>
                        <div className="flex items-center gap-1">
                          <span className="text-muted-foreground">Type :</span>
                          <span>{alertTypeLabel(alert.alert_type)}</span>
                          {alert.alert_type === 'H' && <AlertTriangle className="h-3.5 w-3.5 text-amber-500" />}
                          {alert.alert_type === 'I' && <CircleX className="h-3.5 w-3.5 text-rose-500" />}
                        </div>
                        {alert.val_error && <p className="text-muted-foreground">{alert.val_error}</p>}
                      </div>
                    </div>
                  ))}
                </div>
                {totalAlerts > alerts.length && (
                  <div className="mt-2 flex justify-center">
                    <Button variant="outline" size="sm" className="text-sm h-7" onClick={loadMoreAlerts} disabled={loadingMoreAlerts}>
                      {loadingMoreAlerts ? 'Loading...' : 'Load more'}
                    </Button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  )
}
