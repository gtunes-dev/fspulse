import { useState, useEffect, useCallback } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, FileX, FolderX, Calendar as CalendarIcon,
  HardDrive, Hash, ShieldAlert, ShieldCheck, ShieldQuestion,
  ChevronDown, Eye, X,
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
import { fetchQuery, countQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateFull, formatScanDate } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'
import { cn } from '@/lib/utils'

// ---- Types ----

interface ItemDetailPanelProps {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
  rootId: number
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
  file_hash: string | null
  val: number | null
  val_error: string | null
  last_hash_scan: number | null
  last_val_scan: number | null
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

function valShort(val: number | null): string {
  switch (val) {
    case 0: return 'V'
    case 1: return 'I'
    case 2: return 'N'
    case 3: return 'U'
    default: return 'U'
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

function hasFieldChanges(v: VersionEntry, prev: VersionEntry): boolean {
  return (
    v.mod_date !== prev.mod_date ||
    v.size !== prev.size ||
    v.file_hash !== prev.file_hash ||
    v.access !== prev.access ||
    v.val !== prev.val ||
    v.val_error !== prev.val_error
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
  const [firstSeenScanId, setFirstSeenScanId] = useState(0)
  const [anchorScanDate, setAnchorScanDate] = useState(0)
  const [openVersions, setOpenVersions] = useState<Record<number, boolean>>({})

  // Alerts state
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [totalAlerts, setTotalAlerts] = useState(0)
  const [loadingMoreAlerts, setLoadingMoreAlerts] = useState(false)

  // Size history state
  const [sizeHistory, setSizeHistory] = useState<SizeHistoryPoint[]>([])
  const [timeWindow, setTimeWindow] = useState<TimeWindowPreset>('3m')
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [loadingSizeHistory, setLoadingSizeHistory] = useState(false)

  // Children counts state
  const [childrenCounts, setChildrenCounts] = useState<ChildrenCounts | null>(null)
  const [loadingChildrenCounts, setLoadingChildrenCounts] = useState(false)

  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath
  const anchorVersion = versions.length > 0 ? versions[0] : null
  const changes = buildChanges(versions)

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
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
          setFirstSeenScanId(data.first_seen_scan_id)
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

  // ---- Badge renderers ----

  const getValidationBadge = (val: number | null) => {
    const short = valShort(val)
    switch (short) {
      case 'V': return <Badge variant="success" className="gap-1 text-xs"><ShieldCheck className="h-2.5 w-2.5" />Valid</Badge>
      case 'I': return <Badge variant="destructive" className="gap-1 text-xs"><ShieldAlert className="h-2.5 w-2.5" />Invalid</Badge>
      case 'N': return <Badge variant="secondary" className="text-xs">No Validator</Badge>
      default: return <Badge variant="secondary" className="gap-1 text-xs"><ShieldQuestion className="h-2.5 w-2.5" />Unknown</Badge>
    }
  }

  const getChangeBadge = (kind: ChangeKind) => {
    const cls = "text-xs"
    switch (kind) {
      case 'initial': return <Badge className={cn("bg-blue-500 hover:bg-blue-600", cls)}>Initial</Badge>
      case 'modified': return <Badge className={cn("bg-amber-500 hover:bg-amber-600", cls)}>Modified</Badge>
      case 'deleted': return <Badge variant="destructive" className={cls}>Deleted</Badge>
      case 'restored': return <Badge variant="success" className={cls}>Restored</Badge>
    }
  }

  const getAlertTypeBadge = (type: string) => {
    const cls = "text-xs"
    switch (type) {
      case 'H': return <Badge variant="destructive" className={cls}>Hash</Badge>
      case 'I': return <Badge variant="destructive" className={cls}>Invalid</Badge>
      case 'A': return <Badge variant="warning" className={cls}>Access</Badge>
      default: return <Badge variant="secondary" className={cls}>{type}</Badge>
    }
  }

  const getAlertStatusBadge = (status: string) => {
    const cls = "text-xs"
    switch (status) {
      case 'O': return <Badge variant="destructive" className={cls}>Open</Badge>
      case 'F': return <Badge className={cn("bg-amber-500 hover:bg-amber-600", cls)}>Flagged</Badge>
      case 'D': return <Badge variant="secondary" className={cls}>Dismissed</Badge>
      default: return <Badge variant="secondary" className={cls}>{status}</Badge>
    }
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
              itemType === 'D' ? <Folder className="h-5 w-5 text-blue-500" /> : <File className="h-5 w-5 text-muted-foreground" />
            )}
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-base font-semibold truncate">{itemName}</p>
          </div>
          <Button variant="ghost" size="sm" className="h-6 w-6 p-0 flex-shrink-0" onClick={onClose}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
        <p className="text-xs text-muted-foreground truncate mt-0.5 pl-7">{itemPath}</p>
        {isTombstone && (
          <div className="mt-1 pl-7">
            <Badge variant="destructive" className="text-xs">Deleted</Badge>
          </div>
        )}
        <div className="text-xs text-muted-foreground mt-1 pl-7">
          {firstSeenScanId > 0 && <>First seen #{firstSeenScanId}</>}
          {firstSeenScanId > 0 && totalVersionCount > 0 && <span className="mx-1">&middot;</span>}
          {totalVersionCount > 0 && <>{totalVersionCount} version{totalVersionCount !== 1 ? 's' : ''}</>}
        </div>
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
              <div className="flex items-center justify-between mb-2">
                <p className="text-sm font-semibold">
                  {scanRef(scanId, anchorScanDate)}
                </p>
                <span className="text-xs text-muted-foreground">v{anchorVersion.version_id}</span>
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
                {itemType === 'F' && (
                  <div>
                    <p className="text-muted-foreground">Validation</p>
                    <div className="mt-0.5">{getValidationBadge(anchorVersion.val)}</div>
                  </div>
                )}
                {itemType === 'F' && anchorVersion.file_hash && (
                  <div className="col-span-2">
                    <p className="text-muted-foreground flex items-center gap-1"><Hash className="h-3 w-3" />Hash</p>
                    <p className="font-mono text-xs break-all mt-0.5">{anchorVersion.file_hash}</p>
                  </div>
                )}
              </div>

              {itemType === 'D' && !isTombstone && (
                <div className="mt-2 pt-2 border-t pl-2">
                  {loadingChildrenCounts ? (
                    <p className="text-sm text-muted-foreground text-center">Loading...</p>
                  ) : childrenCounts && (childrenCounts.file_count > 0 || childrenCounts.directory_count > 0) ? (
                    <div className="flex items-center justify-center gap-4 text-sm">
                      <span className="flex items-center gap-1">
                        <Folder className="h-3 w-3" style={{ color: 'hsl(142 71% 45%)' }} />
                        <span className="font-medium">{childrenCounts.directory_count.toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1">
                        <File className="h-3 w-3" style={{ color: 'hsl(221 83% 53%)' }} />
                        <span className="font-medium">{childrenCounts.file_count.toLocaleString()}</span>
                      </span>
                    </div>
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
                    const isExpandable = change.kind === 'modified' && change.prev && hasFieldChanges(v, change.prev)

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
                              {getChangeBadge(change.kind)}
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
                                {change.prev && v.file_hash !== change.prev.file_hash && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium flex items-center gap-1"><Hash className="h-2.5 w-2.5" />Hash</p>
                                    <p className="font-mono break-all text-muted-foreground">{change.prev.file_hash || 'N/A'}</p>
                                    <p className="font-mono break-all">{v.file_hash || 'N/A'}</p>
                                  </div>
                                )}
                                {change.prev && v.access !== change.prev.access && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Access</p>
                                    <p className="text-muted-foreground">{accessLabel(change.prev.access)} &rarr; {accessLabel(v.access)}</p>
                                  </div>
                                )}
                                {change.prev && v.val !== change.prev.val && (
                                  <div className="bg-muted/50 p-1.5 rounded">
                                    <p className="font-medium">Validation</p>
                                    <div className="flex items-center gap-1 mt-0.5">{getValidationBadge(change.prev.val)} <span>&rarr;</span> {getValidationBadge(v.val)}</div>
                                  </div>
                                )}
                              </div>
                            </CollapsibleContent>
                          </Collapsible>
                        ) : (
                          <div className="flex items-center gap-1.5">
                            <div className="h-4 w-4 flex-shrink-0" />
                            {getChangeBadge(change.kind)}
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
                    <div key={alert.alert_id} className="px-2 py-1.5 space-y-1">
                      <div className="flex items-center gap-1.5">
                        {getAlertTypeBadge(alert.alert_type)}
                        {getAlertStatusBadge(alert.alert_status)}
                        <span className="text-xs text-muted-foreground">#{alert.scan_id}</span>
                      </div>
                      {alert.val_error && <p className="text-xs text-red-600">{alert.val_error}</p>}
                      <p className="text-xs text-muted-foreground">{formatDateFull(alert.created)}</p>
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
