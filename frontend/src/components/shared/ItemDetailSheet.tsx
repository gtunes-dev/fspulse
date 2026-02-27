import { useState, useEffect, useCallback } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, FileX, FolderX, Calendar as CalendarIcon,
  HardDrive, Hash, ShieldAlert, ShieldCheck, ShieldQuestion,
  ChevronDown, Eye,
} from 'lucide-react'
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import { Calendar } from '@/components/ui/calendar'
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
  Legend,
} from 'recharts'
import { fetchQuery, countQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateFull, formatScanDate } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'
import { cn } from '@/lib/utils'

// ---- Types ----

interface ItemDetailSheetProps {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
  rootId: number
  scanId: number
  open: boolean
  onOpenChange: (open: boolean) => void
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

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y' | 'custom'

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

/** Classify a version relative to its predecessor */
function classifyChange(version: VersionEntry, prev: VersionEntry | null): ChangeKind {
  if (!prev) return 'initial'
  if (version.is_deleted && !prev.is_deleted) return 'deleted'
  if (!version.is_deleted && prev.is_deleted) return 'restored'
  return 'modified'
}

/** Build the list of VersionChange entries from raw versions (ordered DESC) */
function buildChanges(versions: VersionEntry[]): VersionChange[] {
  if (versions.length === 0) return []

  const changes: VersionChange[] = []
  for (let i = 0; i < versions.length; i++) {
    const version = versions[i]
    const prev = i + 1 < versions.length ? versions[i + 1] : null
    changes.push({
      version,
      kind: classifyChange(version, prev),
      prev,
    })
  }
  return changes
}

/** Check if two versions differ in any visible field */
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

export function ItemDetailSheet({
  itemId,
  itemPath,
  itemType,
  isTombstone,
  scanId,
  open,
  onOpenChange,
}: ItemDetailSheetProps) {
  // Version history state
  const [versions, setVersions] = useState<VersionEntry[]>([])
  const [loadingVersions, setLoadingVersions] = useState(false)
  const [loadingMoreVersions, setLoadingMoreVersions] = useState(false)
  const [hasMoreVersions, setHasMoreVersions] = useState(false)
  const [totalVersionCount, setTotalVersionCount] = useState(0)
  const [firstSeenScanId, setFirstSeenScanId] = useState(0)
  const [firstSeenScanDate, setFirstSeenScanDate] = useState(0)
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
  const [toDate, setToDate] = useState<Date | undefined>()
  const [loadingSizeHistory, setLoadingSizeHistory] = useState(false)

  // Children counts state (for directories)
  const [childrenCounts, setChildrenCounts] = useState<ChildrenCounts | null>(null)
  const [loadingChildrenCounts, setLoadingChildrenCounts] = useState(false)

  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath

  // The anchor version is always the first in our DESC-ordered list
  const anchorVersion = versions.length > 0 ? versions[0] : null

  // Build change entries from loaded versions
  const changes = buildChanges(versions)

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
  }, [itemId])

  // ---- Data loading ----

  useEffect(() => {
    if (!open) return

    async function loadData() {
      setLoadingVersions(true)
      try {
        // Load version history (anchored at scanId)
        const versionResponse = await fetch(
          `/api/items/${itemId}/version-history?scan_id=${scanId}&limit=${VERSIONS_PER_PAGE}`
        )
        if (versionResponse.ok) {
          const data: VersionHistoryInitResponse = await versionResponse.json()
          setVersions(data.versions)
          setHasMoreVersions(data.has_more)
          setTotalVersionCount(data.total_count)
          setFirstSeenScanId(data.first_seen_scan_id)
          setFirstSeenScanDate(data.first_seen_scan_date)
          setAnchorScanDate(data.anchor_scan_date)
        }

        // Load alerts
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
  }, [open, itemId, scanId])

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
      const newAlerts = alertResponse.rows.map(parseAlertRow)
      setAlerts(prev => [...prev, ...newAlerts])
    } catch (error) {
      console.error('Error loading more alerts:', error)
    } finally {
      setLoadingMoreAlerts(false)
    }
  }

  // ---- Size history ----

  const getDateRangeForPreset = (preset: TimeWindowPreset): { from: Date; to: Date } => {
    const now = new Date()
    const today = startOfDay(now)
    switch (preset) {
      case '7d': return { from: subDays(today, 7), to: today }
      case '30d': return { from: subDays(today, 30), to: today }
      case '3m': return { from: subMonths(today, 3), to: today }
      case '6m': return { from: subMonths(today, 6), to: today }
      case '1y': return { from: subYears(today, 1), to: today }
      case 'custom': return {
        from: fromDate || subMonths(today, 3),
        to: toDate || today,
      }
    }
  }

  useEffect(() => {
    if (timeWindow !== 'custom') {
      const { from, to } = getDateRangeForPreset(timeWindow)
      setFromDate(from)
      setToDate(to)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [timeWindow])

  useEffect(() => {
    const today = startOfDay(new Date())
    setFromDate(subMonths(today, 3))
    setToDate(today)
  }, [])

  const loadSizeHistory = useCallback(async () => {
    if (!open || !fromDate) return

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
  }, [itemId, fromDate, scanId, open])

  useEffect(() => {
    loadSizeHistory()
  }, [loadSizeHistory])

  // ---- Children counts (directories) ----

  useEffect(() => {
    async function loadChildrenCounts() {
      if (!open || itemType !== 'D' || isTombstone) {
        setChildrenCounts(null)
        return
      }

      setLoadingChildrenCounts(true)
      try {
        const response = await fetch(
          `/api/items/${itemId}/children-counts?scan_id=${scanId}`
        )
        if (response.ok) {
          const data = await response.json()
          setChildrenCounts({
            file_count: data.file_count,
            directory_count: data.directory_count,
          })
        } else {
          setChildrenCounts(null)
        }
      } catch (error) {
        console.error('Error loading children counts:', error)
        setChildrenCounts(null)
      } finally {
        setLoadingChildrenCounts(false)
      }
    }

    loadChildrenCounts()
  }, [open, itemId, itemType, isTombstone, scanId])

  // ---- Badge renderers ----

  const getValidationBadge = (val: number | null) => {
    const short = valShort(val)
    switch (short) {
      case 'V':
        return <Badge variant="success" className="gap-1"><ShieldCheck className="h-3 w-3" />Valid</Badge>
      case 'I':
        return <Badge variant="destructive" className="gap-1"><ShieldAlert className="h-3 w-3" />Invalid</Badge>
      case 'N':
        return <Badge variant="secondary" className="gap-1">No Validator</Badge>
      case 'U':
      default:
        return <Badge variant="secondary" className="gap-1"><ShieldQuestion className="h-3 w-3" />Unknown</Badge>
    }
  }

  const getChangeBadge = (kind: ChangeKind) => {
    switch (kind) {
      case 'initial':
        return <Badge className="bg-blue-500 hover:bg-blue-600">Initial Version</Badge>
      case 'modified':
        return <Badge className="bg-amber-500 hover:bg-amber-600">Modified</Badge>
      case 'deleted':
        return <Badge variant="destructive">Deleted</Badge>
      case 'restored':
        return <Badge variant="success">Restored</Badge>
    }
  }

  const getAlertTypeBadge = (type: string) => {
    switch (type) {
      case 'H':
        return <Badge variant="destructive">Suspicious Hash</Badge>
      case 'I':
        return <Badge variant="destructive">Invalid Item</Badge>
      case 'A':
        return <Badge variant="warning">Access Denied</Badge>
      default:
        return <Badge variant="secondary">{type}</Badge>
    }
  }

  const getAlertStatusBadge = (status: string) => {
    switch (status) {
      case 'O':
        return <Badge variant="destructive">Open</Badge>
      case 'F':
        return <Badge className="bg-amber-500 hover:bg-amber-600">Flagged</Badge>
      case 'D':
        return <Badge variant="secondary">Dismissed</Badge>
      default:
        return <Badge variant="secondary">{status}</Badge>
    }
  }

  // ---- Scan reference helpers ----

  const scanRef = (scanId: number, scanDate: number) => (
    <>Scan <span className="font-mono font-semibold">#{scanId}</span> ({formatScanDate(scanDate)})</>
  )

  const scanRangeLabel = (v: VersionEntry) => {
    if (v.first_scan_id === v.last_scan_id) {
      return scanRef(v.first_scan_id, v.first_scan_date)
    }
    return (
      <>
        Scans <span className="font-mono font-semibold">#{v.first_scan_id}</span>{' '}
        ({formatScanDate(v.first_scan_date)}) &ndash;{' '}
        <span className="font-mono font-semibold">#{v.last_scan_id}</span>{' '}
        ({formatScanDate(v.last_scan_date)})
      </>
    )
  }

  // ---- Render ----

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="!w-[650px] sm:!w-[700px] !max-w-[700px] overflow-y-auto">
        <SheetHeader className="space-y-4">
          <div className="flex items-start gap-4">
            <div className="flex-shrink-0">
              {isTombstone ? (
                itemType === 'D' ? (
                  <FolderX className="h-12 w-12 text-destructive" />
                ) : (
                  <FileX className="h-12 w-12 text-destructive" />
                )
              ) : (
                itemType === 'D' ? (
                  <Folder className="h-12 w-12 text-blue-500" />
                ) : (
                  <File className="h-12 w-12 text-muted-foreground" />
                )
              )}
            </div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-2xl font-bold break-words">{itemName}</SheetTitle>
              <p className="text-sm text-muted-foreground break-all mt-1">{itemPath}</p>
              <p className="text-xs text-muted-foreground mt-1">
                {firstSeenScanId > 0 && (
                  <>First seen {scanRef(firstSeenScanId, firstSeenScanDate)}</>
                )}
                {firstSeenScanId > 0 && totalVersionCount > 0 && (
                  <span className="mx-1.5">&middot;</span>
                )}
                {totalVersionCount > 0 && (
                  <>{totalVersionCount.toLocaleString()} version{totalVersionCount !== 1 ? 's' : ''}</>
                )}
              </p>
              {isTombstone && (
                <div className="mt-2 flex items-center gap-2">
                  <Badge variant="destructive" className="text-base px-3 py-1">Deleted Item</Badge>
                  <span className="text-sm text-muted-foreground">This item no longer exists</span>
                </div>
              )}
            </div>
          </div>
        </SheetHeader>

        {loadingVersions ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading details...</p>
          </div>
        ) : (
          <div className="space-y-6 mt-6">
            {/* Current State Card */}
            {anchorVersion && (
              <Card className="border-2">
                <CardHeader>
                  <div className="flex items-center justify-between">
                    <CardTitle className="text-base">
                      {scanRef(scanId, anchorScanDate)}
                    </CardTitle>
                    <span className="text-xs text-muted-foreground">
                      Version #{anchorVersion.version_id}
                    </span>
                  </div>
                </CardHeader>
                <CardContent>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Item ID</p>
                      <p className="text-base font-semibold mt-1 font-mono">{itemId}</p>
                    </div>
                    <div>
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <HardDrive className="h-4 w-4" />
                        Type
                      </p>
                      <p className="text-base font-semibold mt-1">{itemTypeLabel(itemType)}</p>
                    </div>
                    <div>
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <CalendarIcon className="h-4 w-4" />
                        Modified
                      </p>
                      <p className="text-base font-semibold mt-1">
                        {anchorVersion.mod_date ? formatDateFull(anchorVersion.mod_date) : 'N/A'}
                      </p>
                    </div>
                    {anchorVersion.size !== null && (
                      <div>
                        <p className="text-sm font-medium text-muted-foreground">Size</p>
                        <p className="text-base font-semibold mt-1">{formatFileSize(anchorVersion.size)}</p>
                      </div>
                    )}
                    {itemType === 'F' && (
                      <div>
                        <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                          <ShieldCheck className="h-4 w-4" />
                          Validation
                        </p>
                        <div className="mt-1">{getValidationBadge(anchorVersion.val)}</div>
                      </div>
                    )}
                    {itemType === 'F' && anchorVersion.file_hash && (
                      <div className="col-span-2">
                        <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                          <Hash className="h-4 w-4" />
                          Hash
                        </p>
                        <p className="text-xs font-mono mt-1 break-all">{anchorVersion.file_hash}</p>
                      </div>
                    )}
                    {itemType === 'F' && anchorVersion.val_error && anchorVersion.val_error.trim() !== '' && (
                      <div className="col-span-2">
                        <p className="text-sm font-medium text-destructive">Validation Error</p>
                        <p className="text-xs font-mono mt-1 bg-destructive/10 p-2 rounded">{anchorVersion.val_error}</p>
                      </div>
                    )}
                  </div>

                  {/* Children counts for directories */}
                  {itemType === 'D' && !isTombstone && (
                    <div className="col-span-2 mt-4 pt-4 border-t">
                      {loadingChildrenCounts ? (
                        <div className="flex items-center justify-center py-4">
                          <p className="text-sm text-muted-foreground">Loading...</p>
                        </div>
                      ) : childrenCounts && (childrenCounts.file_count > 0 || childrenCounts.directory_count > 0) ? (
                        <div className="flex items-center justify-center gap-6">
                          <div className="flex items-center gap-2">
                            <Folder className="h-4 w-4" style={{ color: 'hsl(142 71% 45%)' }} />
                            <span className="text-base font-semibold">
                              {childrenCounts.directory_count.toLocaleString()}
                            </span>
                          </div>
                          <div className="flex items-center gap-2">
                            <File className="h-4 w-4" style={{ color: 'hsl(221 83% 53%)' }} />
                            <span className="text-base font-semibold">
                              {childrenCounts.file_count.toLocaleString()}
                            </span>
                          </div>
                        </div>
                      ) : (
                        <div className="flex items-center justify-center py-4">
                          <p className="text-sm text-muted-foreground">No items in this directory</p>
                        </div>
                      )}
                    </div>
                  )}
                </CardContent>
              </Card>
            )}

            {/* Size History Section */}
            <Card>
                <CardHeader>
                  <div className="flex items-center justify-between flex-wrap gap-4">
                    <CardTitle>Size History</CardTitle>
                    <div className="flex items-center gap-2">
                      <Select value={timeWindow} onValueChange={(v) => setTimeWindow(v as TimeWindowPreset)}>
                        <SelectTrigger className="w-[140px]">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="7d">Last 7 Days</SelectItem>
                          <SelectItem value="30d">Last 30 Days</SelectItem>
                          <SelectItem value="3m">Last 3 Months</SelectItem>
                          <SelectItem value="6m">Last 6 Months</SelectItem>
                          <SelectItem value="1y">Last Year</SelectItem>
                          <SelectItem value="custom">Custom Range</SelectItem>
                        </SelectContent>
                      </Select>
                      {timeWindow === 'custom' && (
                        <>
                          <Popover>
                            <PopoverTrigger asChild>
                              <Button
                                variant="outline"
                                className={cn("w-[140px] justify-start text-left font-normal", !fromDate && "text-muted-foreground")}
                              >
                                <CalendarIcon className="mr-2 h-4 w-4" />
                                {fromDate ? format(fromDate, "MMM dd, yyyy") : "From"}
                              </Button>
                            </PopoverTrigger>
                            <PopoverContent className="w-auto p-0" align="start">
                              <Calendar mode="single" selected={fromDate} onSelect={setFromDate} />
                            </PopoverContent>
                          </Popover>
                          <Popover>
                            <PopoverTrigger asChild>
                              <Button
                                variant="outline"
                                className={cn("w-[140px] justify-start text-left font-normal", !toDate && "text-muted-foreground")}
                              >
                                <CalendarIcon className="mr-2 h-4 w-4" />
                                {toDate ? format(toDate, "MMM dd, yyyy") : "To"}
                              </Button>
                            </PopoverTrigger>
                            <PopoverContent className="w-auto p-0" align="start">
                              <Calendar mode="single" selected={toDate} onSelect={setToDate} />
                            </PopoverContent>
                          </Popover>
                        </>
                      )}
                    </div>
                  </div>
                </CardHeader>
                <CardContent>
                  {loadingSizeHistory ? (
                    <div className="border border-border rounded-lg">
                      <div className="flex items-center justify-center h-64">
                        <p className="text-muted-foreground">Loading history...</p>
                      </div>
                    </div>
                  ) : sizeHistory.length === 0 ? (
                    <div className="border border-border rounded-lg">
                      <div className="flex items-center justify-center h-64">
                        <p className="text-sm text-muted-foreground">
                          No size history available for this time range
                        </p>
                      </div>
                    </div>
                  ) : (
                    <div className="border border-border rounded-lg p-4">
                      <ChartContainer
                        config={{
                          size: {
                            label: 'Size',
                            color: 'hsl(271 81% 56%)',
                          },
                        }}
                        className="aspect-auto h-[300px]"
                      >
                        <LineChart
                          data={sizeHistory.map((point) => ({
                            date: format(new Date(point.started_at * 1000), 'MMM dd'),
                            size: point.size,
                          }))}
                        >
                          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                          <XAxis
                            dataKey="date"
                            tick={{ fill: 'hsl(var(--muted-foreground))' }}
                          />
                          <YAxis
                            tick={{ fill: 'hsl(var(--muted-foreground))' }}
                            tickFormatter={(value) => {
                              const bytes = value as number
                              const units = ['B', 'KB', 'MB', 'GB', 'TB']
                              let i = 0
                              let size = bytes
                              while (size >= 1024 && i < units.length - 1) {
                                size /= 1024
                                i++
                              }
                              return `${size.toFixed(1)} ${units[i]}`
                            }}
                          />
                          <ChartTooltip
                            content={<ChartTooltipContent />}
                            formatter={(value) => {
                              const bytes = value as number
                              return formatFileSize(bytes)
                            }}
                          />
                          <Legend />
                          <Line
                            type="step"
                            dataKey="size"
                            stroke="var(--color-size)"
                            strokeWidth={2}
                            dot={false}
                            name="Size"
                          />
                        </LineChart>
                      </ChartContainer>
                    </div>
                  )}
                </CardContent>
            </Card>

            {/* Version History Section */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle>Version History</CardTitle>
                  {totalVersionCount > 0 && (
                    <p className="text-sm text-muted-foreground">
                      Showing {versions.length} of {totalVersionCount.toLocaleString()} version{totalVersionCount !== 1 ? 's' : ''}
                    </p>
                  )}
                </div>
              </CardHeader>
              <CardContent className="p-6">
                {changes.length === 0 ? (
                  <div className="border border-border rounded-lg">
                    <p className="text-sm text-muted-foreground text-center py-12">
                      No version history available for this item
                    </p>
                  </div>
                ) : (
                  <>
                    <div className="border border-border rounded-lg">
                      <div className="p-0">
                        {changes.map((change, idx) => {
                          const v = change.version
                          const isAnchor = idx === 0
                          const isOpen = openVersions[v.version_id] || false
                          const setIsOpen = (open: boolean) => {
                            setOpenVersions(prev => ({ ...prev, [v.version_id]: open }))
                          }

                          // Determine if this entry is expandable
                          const isExpandable =
                            change.kind === 'modified' && change.prev && hasFieldChanges(v, change.prev)

                          return (
                            <div key={v.version_id}>
                              <div className={cn("p-4", isAnchor && "bg-accent/30")}>
                                {isExpandable ? (
                                  <Collapsible open={isOpen} onOpenChange={setIsOpen}>
                                    <div className="flex items-center gap-2">
                                      <CollapsibleTrigger asChild>
                                        <Button variant="ghost" size="icon" className="h-5 w-5 p-0 flex-shrink-0">
                                          <ChevronDown
                                            className={`h-3 w-3 transition-transform duration-200 ${isOpen ? '' : '-rotate-90'}`}
                                          />
                                        </Button>
                                      </CollapsibleTrigger>
                                      {getChangeBadge(change.kind)}
                                      <p className="text-xs text-muted-foreground">
                                        {scanRangeLabel(v)}
                                        <span className="mx-2">&bull;</span>
                                        Version <span className="font-mono font-semibold">#{v.version_id}</span>
                                      </p>
                                      {isAnchor && (
                                        <Eye className="h-3.5 w-3.5 text-primary ml-auto flex-shrink-0" aria-label="Current view" />
                                      )}
                                    </div>
                                    <CollapsibleContent className="mt-2 ml-7">
                                      <div className="space-y-2 text-xs">
                                        {change.prev && v.mod_date !== change.prev.mod_date && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <CalendarIcon className="h-3 w-3" />
                                              Modification Date
                                            </p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">
                                                {change.prev.mod_date ? formatDateFull(change.prev.mod_date) : 'N/A'}
                                              </span>
                                              <span className="text-muted-foreground">&rarr;</span>
                                              <span className="font-medium">
                                                {v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}
                                              </span>
                                            </div>
                                          </div>
                                        )}

                                        {change.prev && v.size !== change.prev.size && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <HardDrive className="h-3 w-3" />
                                              File Size
                                            </p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">
                                                {formatFileSize(change.prev.size)}
                                              </span>
                                              <span className="text-muted-foreground">&rarr;</span>
                                              <span className="font-medium">
                                                {formatFileSize(v.size)}
                                              </span>
                                            </div>
                                          </div>
                                        )}

                                        {change.prev && v.file_hash !== change.prev.file_hash && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <Hash className="h-3 w-3" />
                                              File Hash
                                            </p>
                                            <div className="space-y-1">
                                              <div className="flex items-start gap-2">
                                                <span className="text-muted-foreground flex-shrink-0">Old:</span>
                                                <span className="font-mono break-all text-muted-foreground">
                                                  {change.prev.file_hash || 'N/A'}
                                                </span>
                                              </div>
                                              <div className="flex items-start gap-2">
                                                <span className="text-muted-foreground flex-shrink-0">New:</span>
                                                <span className="font-mono break-all font-medium">
                                                  {v.file_hash || 'N/A'}
                                                </span>
                                              </div>
                                            </div>
                                          </div>
                                        )}

                                        {change.prev && v.access !== change.prev.access && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1">Access</p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">{accessLabel(change.prev.access)}</span>
                                              <span className="text-muted-foreground">&rarr;</span>
                                              <span className="font-medium">{accessLabel(v.access)}</span>
                                            </div>
                                          </div>
                                        )}

                                        {change.prev && v.val !== change.prev.val && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <ShieldCheck className="h-3 w-3" />
                                              Validation Status
                                            </p>
                                            <div className="flex items-center gap-2">
                                              {getValidationBadge(change.prev.val)}
                                              <span className="text-muted-foreground">&rarr;</span>
                                              {getValidationBadge(v.val)}
                                            </div>
                                          </div>
                                        )}

                                        {change.prev && v.val_error !== change.prev.val_error && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 text-destructive">Validation Error</p>
                                            <div className="space-y-1">
                                              {change.prev.val_error && (
                                                <p className="font-mono text-muted-foreground break-all">
                                                  Old: {change.prev.val_error}
                                                </p>
                                              )}
                                              <p className="font-mono font-medium break-all">
                                                {v.val_error ? `New: ${v.val_error}` : 'Cleared'}
                                              </p>
                                            </div>
                                          </div>
                                        )}
                                      </div>
                                    </CollapsibleContent>
                                  </Collapsible>
                                ) : (
                                  // Non-expandable entry (initial, deleted, restored, or modification with no visible diff)
                                  <div className="flex items-center gap-2">
                                    <div className="h-5 w-5 flex-shrink-0" />
                                    {getChangeBadge(change.kind)}
                                    <p className="text-xs text-muted-foreground">
                                      {scanRangeLabel(v)}
                                      <span className="mx-2">&bull;</span>
                                      Version <span className="font-mono font-semibold">#{v.version_id}</span>
                                    </p>
                                    {isAnchor && (
                                      <Eye className="h-3.5 w-3.5 text-primary ml-auto flex-shrink-0" aria-label="Current view" />
                                    )}
                                  </div>
                                )}
                              </div>
                              {idx < changes.length - 1 && <Separator />}
                            </div>
                          )
                        })}
                      </div>
                    </div>
                    {hasMoreVersions && (
                      <div className="mt-4 flex justify-center">
                        <Button
                          variant="outline"
                          onClick={loadMoreVersions}
                          disabled={loadingMoreVersions}
                        >
                          {loadingMoreVersions ? 'Loading...' : 'Load older versions'}
                        </Button>
                      </div>
                    )}
                  </>
                )}
              </CardContent>
            </Card>

            {/* Alerts Section */}
            <Card>
              <CardHeader>
                <div className="flex items-center justify-between">
                  <CardTitle>Alerts</CardTitle>
                  {totalAlerts > ALERTS_PER_PAGE && (
                    <p className="text-sm text-muted-foreground">
                      Showing {alerts.length} of {totalAlerts} alert{totalAlerts !== 1 ? 's' : ''}
                    </p>
                  )}
                </div>
              </CardHeader>
              <CardContent className="p-6">
                {totalAlerts === 0 ? (
                  <div className="border border-border rounded-lg">
                    <p className="text-sm text-muted-foreground text-center py-12">
                      No alerts for this item
                    </p>
                  </div>
                ) : (
                  <>
                    <div className="border border-border rounded-lg">
                      <div className="p-0">
                        {alerts.map((alert, idx) => (
                          <div key={alert.alert_id}>
                            <div className="p-4">
                              <div className="space-y-2">
                                <div className="flex items-center gap-2">
                                  {getAlertTypeBadge(alert.alert_type)}
                                  {getAlertStatusBadge(alert.alert_status)}
                                  <p className="text-xs text-muted-foreground">
                                    Scan <span className="font-mono font-semibold">#{alert.scan_id}</span>
                                  </p>
                                </div>
                                {alert.val_error && (
                                  <p className="text-sm text-red-600">{alert.val_error}</p>
                                )}
                                <p className="text-xs text-muted-foreground">
                                  Created on {formatDateFull(alert.created)}
                                </p>
                              </div>
                            </div>
                            {idx < alerts.length - 1 && <Separator />}
                          </div>
                        ))}
                      </div>
                    </div>
                    {totalAlerts > alerts.length && alerts.length >= ALERTS_PER_PAGE && (
                      <div className="mt-4 flex justify-center">
                        <Button
                          variant="outline"
                          onClick={loadMoreAlerts}
                          disabled={loadingMoreAlerts}
                        >
                          {loadingMoreAlerts ? 'Loading...' : `Load ${Math.min(ALERTS_PER_PAGE, totalAlerts - alerts.length)} more`}
                        </Button>
                      </div>
                    )}
                  </>
                )}
              </CardContent>
            </Card>
          </div>
        )}
      </SheetContent>
    </Sheet>
  )
}
