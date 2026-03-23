import { useState, useEffect, useCallback, type ReactNode } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, FileX, FolderX, Calendar as CalendarIcon,
  HardDrive, AlertTriangle, CircleX, ChevronDown, ChevronLeft, ChevronRight, Eye, X,
  ShieldCheck, ShieldOff,
} from 'lucide-react'
import { Switch } from '@/components/ui/switch'
import { setDoNotValidate } from '@/lib/api'
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
  // Callback when item data is mutated (e.g. do_not_validate toggled)
  onItemChanged?: () => void
}

interface VersionEntry {
  item_version: number
  first_scan_id: number
  last_scan_id: number
  first_scan_date: number
  last_scan_date: number
  is_added: boolean
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

interface VersionPageResponse {
  versions: VersionEntry[]
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
  do_not_validate: boolean
  hash_state: number | null
  file_hash: string | null
  val_state: number | null
  val_error: string | null
}

type ChangeKind = 'initial' | 'modified' | 'deleted' | 'restored'

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y' | 'custom'

// ---- Constants ----

const VERSIONS_PER_PAGE = 10

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

function classifyChange(v: VersionEntry, prev: VersionEntry | null): ChangeKind {
  if (v.is_added) return 'initial'
  if (v.is_deleted && prev && !prev.is_deleted) return 'deleted'
  if (v.is_deleted) return 'deleted'
  if (!v.is_deleted && prev?.is_deleted) return 'restored'
  return 'modified'
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

/** Build a predecessor map from a set of versions (which may include one extra for boundary). */
function buildPredecessorMap(allVersions: VersionEntry[]): Map<number, VersionEntry> {
  const byVersion = new Map<number, VersionEntry>()
  for (const v of allVersions) byVersion.set(v.item_version, v)
  const predecessors = new Map<number, VersionEntry>()
  for (const v of allVersions) {
    const prev = byVersion.get(v.item_version - 1)
    if (prev) predecessors.set(v.item_version, prev)
  }
  return predecessors
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
  onItemChanged,
}: ItemDetailProps) {
  // Version history state
  const [versions, setVersions] = useState<VersionEntry[]>([])
  const [predecessors, setPredecessors] = useState<Map<number, VersionEntry>>(new Map())
  const [loadingVersions, setLoadingVersions] = useState(false)
  const [totalVersionCount, setTotalVersionCount] = useState(0)
  const [versionPage, setVersionPage] = useState(0)
  const [versionOrder, setVersionOrder] = useState<'desc' | 'asc'>('desc')
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

  // Selected version for the detail view — stored as full data, not derived from page
  const [selectedVersion, setSelectedVersion] = useState<VersionEntry | null>(null)

  const isPanel = mode === 'panel'
  const isSheet = mode === 'sheet'
  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath

  // For sheet mode, skip data loading when not open
  const shouldLoad = isPanel || open === true

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
    setPathExpanded(false)
    setSelectedVersion(null)
  }, [itemId])

  // ---- Data loading ----

  // Load version count (once per item)
  useEffect(() => {
    if (!shouldLoad) return
    async function loadCount() {
      try {
        const response = await fetch(`/api/items/${itemId}/version-count`)
        if (response.ok) {
          const data = await response.json()
          setTotalVersionCount(data.total)
        }
      } catch (error) {
        console.error('Error loading version count:', error)
      }
    }
    loadCount()
  }, [shouldLoad, itemId])

  // Load a page of versions with one extra record for predecessor diffs.
  // If selectVersion is provided, select it after loading.
  const loadVersionPage = useCallback(async (
    page: number,
    order: 'asc' | 'desc',
    selectVersion?: number,
  ) => {
    if (!shouldLoad) return
    try {
      const pageOffset = page * VERSIONS_PER_PAGE

      // Over-request by 1 to get the predecessor for the boundary version.
      // For desc: extra record is after the page (lower version number)
      // For asc: extra record is before the page (lower version number)
      const fetchOffset = order === 'asc' && pageOffset > 0 ? pageOffset - 1 : pageOffset
      const fetchLimit = (order === 'asc' && pageOffset > 0)
        ? VERSIONS_PER_PAGE + 1
        : VERSIONS_PER_PAGE + 1

      const response = await fetch(
        `/api/items/${itemId}/versions?offset=${fetchOffset}&limit=${fetchLimit}&order=${order}`
      )
      if (response.ok) {
        const data: VersionPageResponse = await response.json()
        const allFetched = data.versions

        // Build predecessor map from all fetched (including the extra)
        setPredecessors(buildPredecessorMap(allFetched))

        // Determine which records to display (exclude the extra)
        let displayVersions: VersionEntry[]
        if (order === 'desc') {
          // Extra record is at the end (lowest version). Display first VERSIONS_PER_PAGE.
          displayVersions = allFetched.slice(0, VERSIONS_PER_PAGE)
        } else {
          // For asc page 0: no predecessor needed, extra is at end
          // For asc page 1+: extra is at start (the predecessor). Display from index 1.
          if (pageOffset > 0 && allFetched.length > VERSIONS_PER_PAGE) {
            displayVersions = allFetched.slice(1, VERSIONS_PER_PAGE + 1)
          } else {
            displayVersions = allFetched.slice(0, VERSIONS_PER_PAGE)
          }
        }

        setVersions(displayVersions)
        setVersionPage(page)

        if (selectVersion !== undefined) {
          const found = displayVersions.find(v => v.item_version === selectVersion)
          if (found) setSelectedVersion(found)
        } else if (!selectedVersion && displayVersions.length > 0) {
          setSelectedVersion(displayVersions[0])
        }
      }
    } catch (error) {
      console.error('Error loading versions:', error)
    }
  }, [shouldLoad, itemId, selectedVersion])

  // Initial load — show loading only on first load
  useEffect(() => {
    if (!shouldLoad) return
    setLoadingVersions(true)
    loadVersionPage(0, versionOrder).finally(() => setLoadingVersions(false))
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shouldLoad, itemId]) // intentionally not including versionOrder or loadVersionPage

  // When sort order changes, compute the page that contains the selected version
  const handleOrderChange = (newOrder: 'asc' | 'desc') => {
    if (newOrder === versionOrder) return
    setVersionOrder(newOrder)
    if (selectedVersion && totalVersionCount > 0) {
      const posInNewOrder = newOrder === 'asc'
        ? selectedVersion.item_version - 1
        : totalVersionCount - selectedVersion.item_version
      const newPage = Math.floor(posInNewOrder / VERSIONS_PER_PAGE)
      loadVersionPage(newPage, newOrder, selectedVersion.item_version)
    } else {
      loadVersionPage(0, newOrder)
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

  const loadIntegrity = useCallback(async () => {
    if (!shouldLoad || itemType !== 'F') {
      setIntegrityState(null)
      return
    }
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
  }, [shouldLoad, itemId, itemType, scanId])

  useEffect(() => {
    loadIntegrity()
  }, [loadIntegrity])

  const handleToggleValidation = async () => {
    if (!integrityState) return
    try {
      await setDoNotValidate(itemId, !integrityState.do_not_validate)
      await loadIntegrity()
      onItemChanged?.()
    } catch {
      // silently fail — integrity state will be stale but not broken
    }
  }

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

  const navigateVersion = (delta: number) => {
    if (!selectedVersion) return
    const newVersionNum = selectedVersion.item_version + delta
    if (newVersionNum < 1 || newVersionNum > totalVersionCount) return
    // If the target version is on the current page, just select it
    const onPage = versions.find(v => v.item_version === newVersionNum)
    if (onPage) {
      setSelectedVersion(onPage)
    } else {
      // Need to load the page containing the target version
      const posInOrder = versionOrder === 'asc'
        ? newVersionNum - 1
        : totalVersionCount - newVersionNum
      const targetPage = Math.floor(posInOrder / VERSIONS_PER_PAGE)
      loadVersionPage(targetPage, versionOrder, newVersionNum)
    }
  }

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
              <p className="text-xs text-muted-foreground mt-1">
                Item <span className="font-mono font-semibold text-foreground">#{itemId}</span>
                {totalVersionCount > 0 && (
                  <>
                    <span className="mx-1.5">&middot;</span>
                    <span>{totalVersionCount} version{totalVersionCount !== 1 ? 's' : ''}</span>
                  </>
                )}
              </p>
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
        <p className="text-xs text-muted-foreground mt-1 pl-7">
          Item <span className="font-mono font-semibold text-foreground">#{itemId}</span>
          {totalVersionCount > 0 && (
            <>
              <span className="mx-1.5">&middot;</span>
              <span>{totalVersionCount} version{totalVersionCount !== 1 ? 's' : ''}</span>
            </>
          )}
        </p>
      </div>
    )
  }

  const renderCurrentState = () => {
    if (!selectedVersion) return null

    const v = selectedVersion
    // Navigator: left = older (lower version number), right = newer (higher version number)
    const canGoOlder = v.item_version > 1
    const canGoNewer = v.item_version < totalVersionCount

    const navigator = (
      <div className="flex items-center gap-1">
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          disabled={!canGoOlder}
          onClick={() => navigateVersion(-1)}
        >
          <ChevronLeft className="h-4 w-4" />
        </Button>
        <span className="text-sm font-medium whitespace-nowrap">
          Version <span className="font-mono">{v.item_version}</span>
          <span className="text-muted-foreground"> of {totalVersionCount}</span>
        </span>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 p-0"
          disabled={!canGoNewer}
          onClick={() => navigateVersion(1)}
        >
          <ChevronRight className="h-4 w-4" />
        </Button>
      </div>
    )

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
            <p className="font-medium">{v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}</p>
          </div>
          {v.size !== null && (
            <div>
              <p className="text-muted-foreground">Size</p>
              <p className="font-medium">{formatFileSize(v.size)}</p>
            </div>
          )}
        </div>

        {/* Integrity (files only) */}
        {itemType === 'F' && integrityState && (
          <div className={`${isPanel ? 'mt-2 pt-2' : 'mt-4 pt-4'} border-t`}>
            <div className="flex items-center justify-between mb-2">
              <p className="text-sm font-semibold">Integrity</p>
              <div className="flex items-center gap-1.5">
                {integrityState.do_not_validate
                  ? <ShieldOff className="h-4 w-4 text-amber-500" />
                  : <ShieldCheck className="h-4 w-4 text-muted-foreground" />
                }
                <Switch
                  size="sm"
                  checked={!integrityState.do_not_validate}
                  onCheckedChange={handleToggleValidation}
                  className="data-[state=checked]:bg-muted-foreground"
                  aria-label={integrityState.do_not_validate ? 'Validation disabled' : 'Validation enabled'}
                />
                <span className="text-xs text-muted-foreground">Validation</span>
              </div>
            </div>
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
                {v && (
                  <div className={`mt-3 text-xs`}>
                    <div className="grid grid-cols-2 gap-x-4 gap-y-1">
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-green-500`} />
                        <span className="text-muted-foreground">Added :</span>
                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-red-500`} />
                        <span className="text-muted-foreground">Deleted :</span>
                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-blue-500`} />
                        <span className="text-muted-foreground">Modified :</span>
                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <span className={`inline-block ${sp.dot} rounded-full bg-zinc-400`} />
                        <span className="text-muted-foreground">Unchanged :</span>
                        <span className="font-medium">{(v.unchanged_count ?? 0).toLocaleString()}</span>
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
        <Card className="border-2 overflow-hidden">
          <div className="flex items-center justify-center bg-muted px-4 py-2">
            {navigator}
          </div>
          <CardHeader className="pt-3">
            <p className="text-xs text-muted-foreground text-center">{scanRangeLabel(v)}</p>
          </CardHeader>
          <CardContent>{stateContent}</CardContent>
        </Card>
      )
    }

    return (
      <div className="px-3 py-3">
        <div className="mb-2">
          {navigator}
          <p className="text-xs text-muted-foreground mt-0.5 pl-7">{scanRangeLabel(v)}</p>
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
    const totalPages = Math.ceil(totalVersionCount / VERSIONS_PER_PAGE)
    const pageStart = versionPage * VERSIONS_PER_PAGE + 1
    const pageEnd = Math.min(pageStart + versions.length - 1, totalVersionCount)

    const sortControl = (
      <Select value={versionOrder} onValueChange={(v) => handleOrderChange(v as 'asc' | 'desc')}>
        <SelectTrigger className={isPanel ? 'h-6 w-[110px] text-xs' : 'h-7 w-[130px] text-xs'}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="desc">Newest first</SelectItem>
          <SelectItem value="asc">Oldest first</SelectItem>
        </SelectContent>
      </Select>
    )

    return (
      <Section mode={mode} title="Version History" trailing={sortControl}>
        {versions.length === 0 ? (
          <p className="text-sm text-muted-foreground text-center py-4">No version history</p>
        ) : (
          <>
            <div className={isSheet ? 'border border-border rounded-lg' : 'divide-y divide-border'}>
              {versions.map((v, idx) => {
                const prev = predecessors.get(v.item_version) ?? null
                const kind = classifyChange(v, prev)
                const isSelected = selectedVersion?.item_version === v.item_version
                const isOpen = openVersions[v.item_version] || false
                const setIsOpen = (val: boolean) => setOpenVersions(prev => ({ ...prev, [v.item_version]: val }))
                const hasIntegrity = v.hash_state != null || v.val_state != null
                const hasMetadataChanges = kind === 'modified' && prev && hasFieldChanges(v, prev)
                const hasInitialFolderCounts = kind === 'initial' && hasNonZeroFolderCounts(v)
                const isExpandable = hasMetadataChanges || hasInitialFolderCounts || hasIntegrity

                const headerContent = (
                  <>
                    <span className="text-xs font-medium shrink-0">v{v.item_version}</span>
                    {getChangeIndicator(kind)}
                    {v.hash_state === 2 && <AlertTriangle className={`${sp.iconLg} text-amber-500 flex-shrink-0`} />}
                    {v.val_state === 2 && <CircleX className={`${sp.iconLg} text-rose-500 flex-shrink-0`} />}
                    <p className="text-xs text-muted-foreground truncate flex-1">
                      {scanRangeLabel(v)}
                    </p>
                    {isSelected && <Eye className={`${sp.iconLg} text-primary flex-shrink-0`} />}
                  </>
                )

                return (
                  <div key={v.item_version}>
                    <div
                      className={cn(
                        sp.padX,
                        "cursor-pointer transition-colors",
                        isSelected ? "bg-accent/50" : "hover:bg-accent/20"
                      )}
                      onClick={() => setSelectedVersion(v)}
                    >
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
                              {/* Metadata diffs (modified versions with predecessor) */}
                              {prev && v.mod_date !== prev.mod_date && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1"><CalendarIcon className={sp.icon} />Modified</p>
                                  <p className="text-muted-foreground">{prev.mod_date ? formatDateFull(prev.mod_date) : 'N/A'} &rarr; {v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}</p>
                                </div>
                              )}
                              {prev && v.size !== prev.size && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium flex items-center gap-1"><HardDrive className={sp.icon} />Size</p>
                                  <p className="text-muted-foreground">{formatFileSize(prev.size)} &rarr; {formatFileSize(v.size)}</p>
                                </div>
                              )}
                              {prev && v.access !== prev.access && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Access</p>
                                  <p className="text-muted-foreground">{accessLabel(prev.access)} &rarr; {accessLabel(v.access)}</p>
                                </div>
                              )}

                              {/* Integrity state */}
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

                              {/* Folder count diffs */}
                              {prev && hasFolderCountChanges(v, prev) && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Folder Counts</p>
                                  <div className="mt-1 space-y-0.5">
                                    {v.add_count !== prev.add_count && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-green-500`} />
                                        <span className="text-muted-foreground">Added:</span>
                                        <span className="text-muted-foreground">{(prev.add_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.delete_count !== prev.delete_count && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-red-500`} />
                                        <span className="text-muted-foreground">Deleted:</span>
                                        <span className="text-muted-foreground">{(prev.delete_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.modify_count !== prev.modify_count && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-blue-500`} />
                                        <span className="text-muted-foreground">Modified:</span>
                                        <span className="text-muted-foreground">{(prev.modify_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.unchanged_count !== prev.unchanged_count && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-zinc-400`} />
                                        <span className="text-muted-foreground">Unchanged:</span>
                                        <span className="text-muted-foreground">{(prev.unchanged_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.unchanged_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                  </div>
                                </div>
                              )}
                              {/* Initial folder counts (no predecessor) */}
                              {!prev && hasNonZeroFolderCounts(v) && (
                                <div className={`bg-muted/50 ${sp.pad} rounded`}>
                                  <p className="font-medium">Folder Counts</p>
                                  <div className="mt-1 space-y-0.5">
                                    {(v.add_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-green-500`} />
                                        <span className="text-muted-foreground">Added:</span>
                                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.delete_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-red-500`} />
                                        <span className="text-muted-foreground">Deleted:</span>
                                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.modify_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-blue-500`} />
                                        <span className="text-muted-foreground">Modified:</span>
                                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.unchanged_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <span className={`inline-block ${sp.dot} rounded-full bg-zinc-400`} />
                                        <span className="text-muted-foreground">Unchanged:</span>
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
                    {isSheet && idx < versions.length - 1 && <Separator />}
                  </div>
                )
              })}
            </div>
            {totalPages > 1 && (
              <div className={`${isPanel ? 'mt-2' : 'mt-3'} flex items-center justify-between text-xs text-muted-foreground`}>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs"
                  disabled={versionPage <= 0}
                  onClick={() => loadVersionPage(versionPage - 1, versionOrder)}
                >
                  ← Prev
                </Button>
                <span>
                  {pageStart}–{pageEnd} of {totalVersionCount}
                </span>
                <Button
                  variant="outline"
                  size="sm"
                  className="h-7 text-xs"
                  disabled={versionPage >= totalPages - 1}
                  onClick={() => loadVersionPage(versionPage + 1, versionOrder)}
                >
                  Next →
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
          {renderVersionHistory()}
          {renderSizeHistory()}
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
