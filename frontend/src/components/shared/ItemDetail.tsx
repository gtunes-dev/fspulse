import { useState, useEffect, useCallback, type ReactNode } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, FileX, FolderX, Calendar as CalendarIcon,
  HardDrive, AlertTriangle, CircleX, ChevronDown, Eye, X,
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
} from 'recharts'
import { Separator } from '@/components/ui/separator'
import { formatDateFull, formatScanDate } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'
import { cn } from '@/lib/utils'

// ---- Types ----

type ItemDetailMode = 'panel' | 'sheet'

interface ItemDetailProps {
  mode: ItemDetailMode
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
  scanId: number | null
  // Panel mode
  onClose?: () => void
  // Sheet mode
  open?: boolean
  onOpenChange?: (open: boolean) => void
}

interface VersionEntry {
  item_version: number
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
  has_more: boolean
  total_count: number
  anchor_scan_date: number
}

interface VersionHistoryPageResponse {
  versions: VersionEntry[]
  has_more: boolean
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

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y' | 'custom'

// ---- Constants ----

const VERSIONS_PER_PAGE = 100

// ---- Helpers ----

function accessLabel(access: number): string {
  switch (access) {
    case 0: return 'No Error'
    case 1: return 'Meta Error'
    case 2: return 'Read Error'
    default: return `Unknown (${access})`
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

function valStateLabel(val: number | null): string {
  switch (val) {
    case 1: return 'Valid'
    case 2: return 'Invalid'
    case 3: return 'No Validator'
    default: return 'Unknown'
  }
}

function hashStateLabel(hash: number | null): string {
  switch (hash) {
    case 1: return 'Baseline'
    case 2: return 'Suspect'
    default: return 'Unknown'
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

// ---- Layout helpers ----

// Section wrapper: Card in sheet mode, div with divider in panel mode
// IMPORTANT: Defined outside ItemDetail so React sees a stable component reference.
// Defining this inside the component causes unmount/remount on every render,
// which breaks Recharts charts that depend on stable DOM measurements.
function Section({ mode, title, trailing, children }: { mode: ItemDetailMode; title: string; trailing?: ReactNode; children: ReactNode }) {
  if (mode === 'sheet') {
    return (
      <Card>
        <CardHeader>
          <div className="flex items-center justify-between">
            <CardTitle>{title}</CardTitle>
            {trailing}
          </div>
        </CardHeader>
        <CardContent>{children}</CardContent>
      </Card>
    )
  }
  return (
    <div className="px-3 py-3">
      <div className="flex items-center justify-between mb-2">
        <p className="text-sm font-semibold">{title}</p>
        {trailing}
      </div>
      {children}
    </div>
  )
}

// ---- Component ----

export function ItemDetail({
  mode,
  itemId,
  itemPath,
  itemType,
  isTombstone,
  scanId,
  onClose,
  open,
  onOpenChange,
}: ItemDetailProps) {
  // Version history state
  const [versions, setVersions] = useState<VersionEntry[]>([])
  const [loadingVersions, setLoadingVersions] = useState(false)
  const [loadingMoreVersions, setLoadingMoreVersions] = useState(false)
  const [hasMoreVersions, setHasMoreVersions] = useState(false)
  const [totalVersionCount, setTotalVersionCount] = useState(0)
  const [anchorScanDate, setAnchorScanDate] = useState(0)
  const [openVersions, setOpenVersions] = useState<Record<number, boolean>>({})

  // Size history state
  const [sizeHistory, setSizeHistory] = useState<SizeHistoryPoint[]>([])
  const [timeWindow, setTimeWindow] = useState<TimeWindowPreset>('3m')
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [toDate, setToDate] = useState<Date | undefined>()
  const [loadingSizeHistory, setLoadingSizeHistory] = useState(false)

  // Children counts state
  const [childrenCounts, setChildrenCounts] = useState<ChildrenCounts | null>(null)
  const [loadingChildrenCounts, setLoadingChildrenCounts] = useState(false)

  // Integrity state (files only)
  const [integrityState, setIntegrityState] = useState<IntegrityState | null>(null)
  const [hashExpanded, setHashExpanded] = useState(false)
  const [pathExpanded, setPathExpanded] = useState(false)

  const isPanel = mode === 'panel'
  const isSheet = mode === 'sheet'
  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath
  const anchorVersion = versions.length > 0 ? versions[0] : null
  const changes = buildChanges(versions)

  // For sheet mode, skip data loading when not open
  const shouldLoad = isPanel || open === true

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
    setPathExpanded(false)
  }, [itemId])

  // ---- Data loading ----

  useEffect(() => {
    if (!shouldLoad) return

    async function loadData() {
      setLoadingVersions(true)
      try {
        const versionResponse = await fetch(
          `/api/items/${itemId}/version-history?${scanId !== null ? `scan_id=${scanId}&` : ''}limit=${VERSIONS_PER_PAGE}`
        )
        if (versionResponse.ok) {
          const data: VersionHistoryInitResponse = await versionResponse.json()
          setVersions(data.versions)
          setHasMoreVersions(data.has_more)
          setTotalVersionCount(data.total_count)
          setAnchorScanDate(data.anchor_scan_date)
        }

      } catch (error) {
        console.error('Error loading item details:', error)
      } finally {
        setLoadingVersions(false)
      }
    }

    loadData()
  }, [shouldLoad, itemId, scanId])

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

  // ---- Size history ----

  const getFromDateForPreset = (preset: TimeWindowPreset): Date => {
    const today = startOfDay(new Date())
    switch (preset) {
      case '7d': return subDays(today, 7)
      case '30d': return subDays(today, 30)
      case '3m': return subMonths(today, 3)
      case '6m': return subMonths(today, 6)
      case '1y': return subYears(today, 1)
      case 'custom': return fromDate || subMonths(today, 3)
    }
  }

  useEffect(() => {
    if (timeWindow !== 'custom') {
      const from = getFromDateForPreset(timeWindow)
      setFromDate(from)
      setToDate(startOfDay(new Date()))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [timeWindow])

  useEffect(() => {
    const today = startOfDay(new Date())
    setFromDate(subMonths(today, 3))
    setToDate(today)
  }, [])

  const loadSizeHistory = useCallback(async () => {
    if (!shouldLoad || !fromDate) return
    setLoadingSizeHistory(true)
    try {
      const fromDateStr = format(fromDate, 'yyyy-MM-dd')
      const response = await fetch(
        `/api/items/${itemId}/size-history?from_date=${fromDateStr}${scanId !== null ? `&to_scan_id=${scanId}` : ''}`
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
  }, [itemId, fromDate, scanId, shouldLoad])

  useEffect(() => {
    loadSizeHistory()
  }, [loadSizeHistory])

  // ---- Children counts ----

  useEffect(() => {
    async function loadChildrenCounts() {
      if (!shouldLoad || itemType !== 'D' || isTombstone) {
        setChildrenCounts(null)
        return
      }
      setLoadingChildrenCounts(true)
      try {
        const response = await fetch(`/api/items/${itemId}/children-counts${scanId !== null ? `?scan_id=${scanId}` : ''}`)
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
  }, [shouldLoad, itemId, itemType, isTombstone, scanId])

  // ---- Integrity state (files only) ----

  useEffect(() => {
    if (!shouldLoad || itemType !== 'F') {
      setIntegrityState(null)
      return
    }
    async function loadIntegrity() {
      try {
        const response = await fetch(`/api/items/${itemId}/integrity-state${scanId !== null ? `?scan_id=${scanId}` : ''}`)
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
  }, [shouldLoad, itemId, itemType, scanId])

  // Spacing constants
  const sp = {
    icon: isPanel ? 'h-2.5 w-2.5' : 'h-3 w-3',
    iconLg: isPanel ? 'h-3 w-3' : 'h-3.5 w-3.5',
    gap: isPanel ? 'gap-1.5' : 'gap-2',
    pad: isPanel ? 'p-1.5' : 'p-2',
    padX: isPanel ? 'px-2 py-1.5' : 'p-4',
    space: isPanel ? 'space-y-1' : 'space-y-2',
    dot: isPanel ? 'w-[7px] h-[7px]' : 'w-2 h-2',
    chevron: isPanel ? 'h-4 w-4' : 'h-5 w-5',
    chevronInner: isPanel ? 'h-2.5 w-2.5' : 'h-3 w-3',
    chartHeight: isPanel ? 'h-[180px]' : 'h-[300px]',
    ml: isPanel ? 'ml-5.5' : 'ml-7',
  }

  // ---- Badge/indicator renderers ----

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
      <span className={`inline-flex items-center ${sp.gap} text-xs flex-shrink-0`}>
        <span className={`inline-block ${sp.dot} rounded-full ${dotColor}`} />
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

  // ---- Content rendering ----

  const renderHeader = () => {
    if (isSheet) {
      return (
        <SheetHeader className="space-y-4">
          <div className="flex items-start gap-4">
            <div className="flex-shrink-0">
              {isTombstone ? (
                itemType === 'D' ? <FolderX className="h-12 w-12 text-destructive" /> : <FileX className="h-12 w-12 text-destructive" />
              ) : (
                itemType === 'D' ? <Folder className="h-12 w-12 text-blue-500" /> : <File className="h-12 w-12 text-muted-foreground" />
              )}
            </div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-2xl font-bold break-words">{itemName}</SheetTitle>
              <button
                className="text-sm text-muted-foreground mt-1 flex items-center gap-1 text-left w-full hover:text-foreground transition-colors"
                onClick={() => setPathExpanded(!pathExpanded)}
              >
                <span className={pathExpanded ? 'break-all' : 'truncate'}>{itemPath}</span>
                <ChevronDown className={cn("h-3 w-3 flex-shrink-0 transition-transform", !pathExpanded && "-rotate-90")} />
              </button>
              {isTombstone && (
                <div className="mt-2">
                  <Badge variant="destructive" className="text-base px-3 py-1">Deleted</Badge>
                </div>
              )}
              {anchorVersion && (
                <p className="text-xs text-muted-foreground mt-1">
                  Item <span className="font-mono font-semibold text-foreground">#{itemId}</span>
                  <span className="mx-1.5">&middot;</span>
                  Version <span className="font-mono font-semibold text-foreground">{anchorVersion.item_version}{totalVersionCount > 0 ? ` of ${totalVersionCount}` : ''}</span>
                </p>
              )}
            </div>
          </div>
        </SheetHeader>
      )
    }

    return (
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
            Version <span className="font-mono font-semibold text-foreground">{anchorVersion.item_version}{totalVersionCount > 0 ? ` of ${totalVersionCount}` : ''}</span>
          </p>
        )}
      </div>
    )
  }

  const renderCurrentState = () => {
    if (!anchorVersion) return null

    const currentStateTitle = isSheet
      ? (scanId !== null ? scanRef(scanId, anchorScanDate) : null)
      : null

    const gridCols = isPanel ? 'grid-cols-2 gap-2' : 'grid-cols-2 gap-4'

    const stateContent = (
      <>
        <div className={`grid ${gridCols} text-sm ${isPanel ? 'pl-2' : ''}`}>
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

        {/* Integrity (files only) */}
        {itemType === 'F' && integrityState && (
          <div className={`${isPanel ? 'mt-2 pt-2' : 'mt-4 pt-4'} border-t`}>
            <p className={`text-sm font-semibold mb-2`}>Integrity</p>
            <div className={`text-sm ${isPanel ? 'pl-2 space-y-2' : 'pl-2 space-y-3'}`}>
              <div>
                <div className="flex items-center gap-1">
                  <span className="text-muted-foreground">Hash :</span>
                  <span>{hashStateLabel(integrityState.hash_state)}</span>
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
                  <span className="text-muted-foreground">Validation :</span>
                  <span>
                    {!integrityState.has_validator && integrityState.val_state == null
                      ? 'No Validator'
                      : valStateLabel(integrityState.val_state)}
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

        {/* Children counts (directories only) */}
        {itemType === 'D' && !isTombstone && (
          <div className={`${isPanel ? 'mt-2 pt-2 pl-2' : 'mt-4 pt-4'} border-t`}>
            {loadingChildrenCounts ? (
              <p className="text-sm text-muted-foreground text-center">Loading...</p>
            ) : childrenCounts && (childrenCounts.file_count > 0 || childrenCounts.directory_count > 0) ? (
              <>
                <div className={`flex items-center justify-center ${isPanel ? 'gap-4' : 'gap-6'} text-sm`}>
                  <span className="flex items-center gap-1">
                    <Folder className={`${isPanel ? 'h-3 w-3' : 'h-4 w-4'} text-muted-foreground`} />
                    <span className="font-medium">{childrenCounts.directory_count.toLocaleString()}</span>
                  </span>
                  <span className="flex items-center gap-1">
                    <File className={`${isPanel ? 'h-3 w-3' : 'h-4 w-4'} text-muted-foreground`} />
                    <span className="font-medium">{childrenCounts.file_count.toLocaleString()}</span>
                  </span>
                </div>
                {anchorVersion && (
                  <div className={`mt-3 text-xs`}>
                    <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-green-500`} />
                        <span className="text-muted-foreground">Added :</span>
                        <span className="font-medium">{(anchorVersion.add_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-red-500`} />
                        <span className="text-muted-foreground">Deleted :</span>
                        <span className="font-medium">{(anchorVersion.delete_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-blue-500`} />
                        <span className="text-muted-foreground">Modified :</span>
                        <span className="font-medium">{(anchorVersion.modify_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-zinc-400`} />
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
      </>
    )

    if (isSheet) {
      return (
        <Card className="border-2">
          <CardHeader>
            <div className="flex items-center justify-between">
              <CardTitle className="text-base">{currentStateTitle}</CardTitle>
            </div>
          </CardHeader>
          <CardContent>{stateContent}</CardContent>
        </Card>
      )
    }

    return (
      <div className="px-3 py-3">
        <div className="mb-2">
          {scanId !== null && <p className="text-sm font-semibold">{scanRef(scanId, anchorScanDate)}</p>}
        </div>
        {stateContent}
      </div>
    )
  }

  const renderSizeHistory = () => {
    const timeWindowSelector = (
      <div className={`flex items-center gap-2 ${isSheet ? 'flex-wrap' : ''}`}>
        <Select value={timeWindow} onValueChange={(v) => setTimeWindow(v as TimeWindowPreset)}>
          <SelectTrigger className={isPanel ? 'h-6 w-[100px] text-xs' : 'w-[140px]'}>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="7d">{isPanel ? '7 Days' : 'Last 7 Days'}</SelectItem>
            <SelectItem value="30d">{isPanel ? '30 Days' : 'Last 30 Days'}</SelectItem>
            <SelectItem value="3m">{isPanel ? '3 Months' : 'Last 3 Months'}</SelectItem>
            <SelectItem value="6m">{isPanel ? '6 Months' : 'Last 6 Months'}</SelectItem>
            <SelectItem value="1y">{isPanel ? '1 Year' : 'Last Year'}</SelectItem>
            <SelectItem value="custom">Custom Range</SelectItem>
          </SelectContent>
        </Select>
        {timeWindow === 'custom' && (
          <>
            <Popover>
              <PopoverTrigger asChild>
                <Button
                  variant="outline"
                  className={cn(
                    isPanel ? 'h-6 w-[120px] text-xs' : 'w-[140px]',
                    'justify-start text-left font-normal',
                    !fromDate && 'text-muted-foreground'
                  )}
                >
                  <CalendarIcon className="mr-2 h-4 w-4" />
                  {fromDate ? format(fromDate, 'MMM dd, yyyy') : 'From'}
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
                  className={cn(
                    isPanel ? 'h-6 w-[120px] text-xs' : 'w-[140px]',
                    'justify-start text-left font-normal',
                    !toDate && 'text-muted-foreground'
                  )}
                >
                  <CalendarIcon className="mr-2 h-4 w-4" />
                  {toDate ? format(toDate, 'MMM dd, yyyy') : 'To'}
                </Button>
              </PopoverTrigger>
              <PopoverContent className="w-auto p-0" align="start">
                <Calendar mode="single" selected={toDate} onSelect={setToDate} />
              </PopoverContent>
            </Popover>
          </>
        )}
      </div>
    )

    const chart = (
      <ChartContainer
        config={{ size: { label: 'Size', color: 'hsl(271 81% 56%)' } }}
        className={`aspect-auto ${sp.chartHeight}`}
      >
        <LineChart data={sizeHistory.map(p => ({ date: format(new Date(p.started_at * 1000), 'MMM dd'), size: p.size }))}>
          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
          <XAxis dataKey="date" tick={{ fill: 'hsl(var(--muted-foreground))', fontSize: isPanel ? 10 : undefined }} />
          <YAxis
            tick={{ fill: 'hsl(var(--muted-foreground))', fontSize: isPanel ? 10 : undefined }}
            tickFormatter={(value) => {
              const units = ['B', 'KB', 'MB', 'GB', 'TB']
              let i = 0; let s = value as number
              while (s >= 1024 && i < units.length - 1) { s /= 1024; i++ }
              return `${s.toFixed(isPanel ? 0 : 1)} ${units[i]}`
            }}
            width={isPanel ? 50 : 60}
          />
          <ChartTooltip content={<ChartTooltipContent />} formatter={(v) => formatFileSize(v as number)} />
          <Line type="step" dataKey="size" stroke="var(--color-size)" strokeWidth={2} dot={false} name="Size" />
        </LineChart>
      </ChartContainer>
    )

    const chartContent = loadingSizeHistory ? (
      <div className={`flex items-center justify-center ${isPanel ? 'h-32' : 'h-64'} text-sm text-muted-foreground`}>Loading...</div>
    ) : sizeHistory.length === 0 ? (
      <div className={`flex items-center justify-center ${isPanel ? 'h-32' : 'h-64'} text-sm text-muted-foreground`}>No size history</div>
    ) : chart

    return (
      <Section mode={mode} title="Size History" trailing={timeWindowSelector}>
        {chartContent}
      </Section>
    )
  }

  const renderVersionHistory = () => {
    const trailing = totalVersionCount > 0 ? (
      <p className="text-xs text-muted-foreground">
        {isSheet
          ? `Showing ${versions.length} of ${totalVersionCount.toLocaleString()} version${totalVersionCount !== 1 ? 's' : ''}`
          : `${versions.length}/${totalVersionCount}`
        }
      </p>
    ) : undefined

    return (
      <Section mode={mode} title="Version History" trailing={trailing}>
        {changes.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-4">No version history</p>
        ) : (
          <>
            <div className={isSheet ? 'border border-border rounded-lg' : 'divide-y divide-border'}>
              {changes.map((change, idx) => {
                const v = change.version
                const isAnchor = changes[0].version.item_version === v.item_version
                const isOpen = openVersions[v.item_version] || false
                const setIsOpen = (val: boolean) => setOpenVersions(prev => ({ ...prev, [v.item_version]: val }))
                const hasIntegrity = v.hash_state != null || v.val_state != null
                const hasMetadataChanges = change.kind === 'modified' && change.prev && hasFieldChanges(v, change.prev)
                const hasInitialFolderCounts = change.kind === 'initial' && hasNonZeroFolderCounts(v)
                const isExpandable = hasMetadataChanges || hasInitialFolderCounts || hasIntegrity
                const prevForCounts = change.prev ?? {
                  add_count: 0, modify_count: 0, delete_count: 0, unchanged_count: 0,
                }

                const headerContent = (
                  <>
                    {getChangeIndicator(change.kind)}
                    {v.hash_state === 2 && <AlertTriangle className={`${sp.iconLg} text-amber-500 flex-shrink-0`} />}
                    {v.val_state === 2 && <CircleX className={`${sp.iconLg} text-rose-500 flex-shrink-0`} />}
                    <p className="text-xs text-muted-foreground truncate flex-1">
                      {scanRangeLabel(v)}
                    </p>
                    {isAnchor && <Eye className={`${sp.iconLg} text-primary flex-shrink-0`} />}
                  </>
                )

                return (
                  <div key={v.item_version}>
                    <div className={cn(sp.padX, isAnchor && "bg-accent/30")}>
                      {isExpandable ? (
                        <Collapsible open={isOpen} onOpenChange={setIsOpen}>
                          <div className={`flex items-center ${sp.gap}`}>
                            <CollapsibleTrigger asChild>
                              <Button variant="ghost" size="icon" className={`${sp.chevron} p-0 flex-shrink-0`}>
                                <ChevronDown className={cn(`${sp.chevronInner} transition-transform`, !isOpen && "-rotate-90")} />
                              </Button>
                            </CollapsibleTrigger>
                            {headerContent}
                          </div>
                          <CollapsibleContent className={`${isPanel ? 'mt-1' : 'mt-2'} ${sp.ml}`}>
                            <div className={`${sp.space} text-xs`}>
                              {change.prev && v.mod_date !== change.prev.mod_date && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1"><CalendarIcon className={sp.icon} />Modified</p>
                                  <p className="text-muted-foreground">{change.prev.mod_date ? formatDateFull(change.prev.mod_date) : 'N/A'} &rarr; {v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}</p>
                                </div>
                              )}
                              {change.prev && v.size !== change.prev.size && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1"><HardDrive className={sp.icon} />Size</p>
                                  <p className="text-muted-foreground">{formatFileSize(change.prev.size)} &rarr; {formatFileSize(v.size)}</p>
                                </div>
                              )}
                              {change.prev && v.access !== change.prev.access && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Access</p>
                                  <p className="text-muted-foreground">{accessLabel(change.prev.access)} &rarr; {accessLabel(v.access)}</p>
                                </div>
                              )}

                              {/* Integrity state (always shown, not as diff) */}
                              {v.file_hash != null ? (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1">
                                    Hash
                                    {v.hash_state === 2 && <AlertTriangle className={`${sp.icon} text-amber-500`} />}
                                  </p>
                                  <p className="font-mono break-all text-muted-foreground">{v.file_hash}</p>
                                </div>
                              ) : v.hash_state != null && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Hash</p>
                                  <p className="text-muted-foreground">Not available</p>
                                </div>
                              )}
                              {v.val_state != null ? (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1">
                                    Validation
                                    {v.val_state === 2 && <CircleX className={`${sp.icon} text-rose-500`} />}
                                  </p>
                                  <p className="text-muted-foreground">{valStateLabel(v.val_state)}</p>
                                  {v.val_error && v.val_error.trim() !== '' && (
                                    <p className="font-mono break-all text-muted-foreground mt-0.5">{v.val_error}</p>
                                  )}
                                </div>
                              ) : v.hash_state != null && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Validation</p>
                                  <p className="text-muted-foreground">Not available</p>
                                </div>
                              )}

                              {/* Folder counts diff */}
                              {((v.add_count ?? 0) !== (prevForCounts.add_count ?? 0) || (v.delete_count ?? 0) !== (prevForCounts.delete_count ?? 0) || (v.modify_count ?? 0) !== (prevForCounts.modify_count ?? 0) || (v.unchanged_count ?? 0) !== (prevForCounts.unchanged_count ?? 0)) && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Folder Counts</p>
                                  <div className="mt-1 space-y-0.5">
                                    {(v.add_count ?? 0) !== (prevForCounts.add_count ?? 0) && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-green-500`} />
                                        <span className="text-muted-foreground">Added :</span>
                                        <span className="text-muted-foreground">{(prevForCounts.add_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.delete_count ?? 0) !== (prevForCounts.delete_count ?? 0) && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-red-500`} />
                                        <span className="text-muted-foreground">Deleted :</span>
                                        <span className="text-muted-foreground">{(prevForCounts.delete_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.modify_count ?? 0) !== (prevForCounts.modify_count ?? 0) && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-blue-500`} />
                                        <span className="text-muted-foreground">Modified :</span>
                                        <span className="text-muted-foreground">{(prevForCounts.modify_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.unchanged_count ?? 0) !== (prevForCounts.unchanged_count ?? 0) && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-zinc-400`} />
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
                        <div className={`flex items-center ${sp.gap}`}>
                          <div className={`${sp.chevron} flex-shrink-0`} />
                          {headerContent}
                        </div>
                      )}
                    </div>
                    {isSheet && idx < changes.length - 1 && <Separator />}
                  </div>
                )
              })}
            </div>
            {hasMoreVersions && (
              <div className={`${isPanel ? 'mt-2' : 'mt-4'} flex justify-center`}>
                <Button variant="outline" size={isPanel ? 'sm' : 'default'} onClick={loadMoreVersions} disabled={loadingMoreVersions}>
                  {loadingMoreVersions ? 'Loading...' : 'Load older'}
                </Button>
              </div>
            )}
          </>
        )}
      </Section>
    )
  }

  // ---- Main render ----

  const content = (
    <>
      {renderHeader()}
      {loadingVersions ? (
        <div className={`flex items-center justify-center ${isPanel ? 'h-32' : 'h-64'} text-muted-foreground text-sm`}>
          Loading...
        </div>
      ) : (
        <div className={isSheet ? 'space-y-6 mt-6' : 'divide-y divide-border border-b border-border'}>
          {renderCurrentState()}
          {renderSizeHistory()}
          {renderVersionHistory()}
        </div>
      )}
    </>
  )

  if (isSheet) {
    return (
      <Sheet open={open} onOpenChange={onOpenChange}>
        <SheetContent side="right" className="!w-[650px] sm:!w-[700px] !max-w-[700px] overflow-y-auto">
          {content}
        </SheetContent>
      </Sheet>
    )
  }

  return <div className="flex flex-col">{content}</div>
}
