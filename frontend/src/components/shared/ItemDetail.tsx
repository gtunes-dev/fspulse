import { useState, useEffect, useCallback, useRef, type ReactNode } from 'react'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
import {
  File, Folder, Calendar as CalendarIcon,
  HardDrive, AlertTriangle, CircleX, ChevronDown, Eye, X,
  ShieldCheck, ShieldOff, Plus, Triangle, Minus,
} from 'lucide-react'
import { Switch } from '@/components/ui/switch'
import { ReviewToggle } from '@/components/shared/ReviewToggle'
import { setDoNotValidate, setIntegrityReviewed } from '@/lib/api'
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
import {
  HoverCard,
  HoverCardTrigger,
  HoverCardContent,
} from '@/components/ui/hover-card'
import {
  type CarouselApi,
  Carousel,
  CarouselContent,
  CarouselItem,
  CarouselPrevious,
  CarouselNext,
} from '@/components/ui/carousel'
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
  val_reviewed_at: number | null
  hash_reviewed_at: number | null
}

interface HashEntry {
  first_scan_id: number
  last_scan_id: number
  scan_started_at: number
  file_hash: string
  hash_state: number
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
const HASHES_PER_PAGE = 10

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
  const [hashHistory, setHashHistory] = useState<HashEntry[]>([])
  const [hashHistoryPage, setHashHistoryPage] = useState(0)
  const [loadingHashHistory, setLoadingHashHistory] = useState(false)
  const [expandedHashes, setExpandedHashes] = useState<Record<number, boolean>>({})
  const [reviewingHash, setReviewingHash] = useState(false)
  const [reviewingVal, setReviewingVal] = useState(false)
  const [pathExpanded, setPathExpanded] = useState(false)
  const [pathTruncated, setPathTruncated] = useState(false)
  const pathRef = useRef<HTMLSpanElement>(null)
  const [carouselApi, setCarouselApi] = useState<CarouselApi>()
  const suppressCarouselSync = useRef(false)

  // Selected version for the detail view — stored as full data, not derived from page
  const [selectedVersion, setSelectedVersion] = useState<VersionEntry | null>(null)

  const isPanel = mode === 'panel'
  const isSheet = mode === 'sheet'
  const lastSlash = itemPath.lastIndexOf('/')
  const itemName = lastSlash >= 0 ? itemPath.slice(lastSlash + 1) : itemPath
  const parentPath = lastSlash > 0 ? itemPath.slice(0, lastSlash) : ''

  // For sheet mode, skip data loading when not open
  const shouldLoad = isPanel || open === true

  // Reset state when switching items
  useEffect(() => {
    setOpenVersions({})
    setPathExpanded(false)
    setPathTruncated(false)
    setSelectedVersion(null)
  }, [itemId])

  // Detect if path text is truncated
  useEffect(() => {
    if (pathExpanded) return
    const el = pathRef.current
    if (el) setPathTruncated(el.scrollWidth > el.clientWidth)
  }, [parentPath, pathExpanded])

  // Sync carousel → selectedVersion when user swipes/clicks arrows
  useEffect(() => {
    if (!carouselApi) return
    const onSelect = () => {
      if (suppressCarouselSync.current) return
      const idx = carouselApi.selectedScrollSnap()
      if (idx >= 0 && idx < versions.length && versions[idx].item_version !== selectedVersion?.item_version) {
        setSelectedVersion(versions[idx])
      }
    }
    carouselApi.on('select', onSelect)
    return () => { carouselApi.off('select', onSelect) }
  }, [carouselApi, versions, selectedVersion])

  // Sync selectedVersion → carousel when user clicks version list
  useEffect(() => {
    if (!carouselApi || !selectedVersion) return
    const idx = versions.findIndex(v => v.item_version === selectedVersion.item_version)
    if (idx >= 0 && idx !== carouselApi.selectedScrollSnap()) {
      suppressCarouselSync.current = true
      carouselApi.scrollTo(idx)
      // Re-enable after animation settles
      requestAnimationFrame(() => { suppressCarouselSync.current = false })
    }
  }, [carouselApi, selectedVersion, versions])

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
  // selectVersion: jump to a specific version after loading.
  // autoSelect: always select the first version (used on initial load to avoid stale closure).
  const loadVersionPage = useCallback(async (
    page: number,
    order: 'asc' | 'desc',
    selectVersion?: number,
    autoSelect?: boolean,
  ) => {
    if (!shouldLoad) return
    try {
      const pageOffset = page * VERSIONS_PER_PAGE

      // Over-request by 1 to get the predecessor for the boundary version.
      const fetchOffset = order === 'asc' && pageOffset > 0 ? pageOffset - 1 : pageOffset
      const fetchLimit = VERSIONS_PER_PAGE + 1

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
          displayVersions = allFetched.slice(0, VERSIONS_PER_PAGE)
        } else {
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
        } else if (autoSelect && displayVersions.length > 0) {
          setSelectedVersion(displayVersions[0])
        }
      }
    } catch (error) {
      console.error('Error loading versions:', error)
    }
  }, [shouldLoad, itemId])

  // Initial load — show loading only on first load; autoSelect first version
  useEffect(() => {
    if (!shouldLoad) return
    setLoadingVersions(true)
    loadVersionPage(0, versionOrder, undefined, true).finally(() => setLoadingVersions(false))
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

  // Load hash history when selected version changes
  const loadHashHistory = useCallback(async () => {
    if (!shouldLoad || !selectedVersion || itemType !== 'F') {
      setHashHistory([])
      return
    }
    setLoadingHashHistory(true)
    try {
      const response = await fetch(
        `/api/items/${itemId}/versions/${selectedVersion.item_version}/hashes`
      )
      if (response.ok) {
        const data = await response.json()
        setHashHistory(data.hashes || [])
      } else {
        setHashHistory([])
      }
    } catch {
      setHashHistory([])
    } finally {
      setLoadingHashHistory(false)
    }
  }, [shouldLoad, itemId, selectedVersion, itemType])

  useEffect(() => {
    setHashHistoryPage(0)
    setExpandedHashes({})
    loadHashHistory()
  }, [loadHashHistory])

  // Review handlers for the selected version
  const handleToggleHashReview = async () => {
    if (!selectedVersion || reviewingHash) return
    setReviewingHash(true)
    try {
      const isReviewed = selectedVersion.hash_reviewed_at != null
      await setIntegrityReviewed(itemId, selectedVersion.item_version, null, !isReviewed)
      // Reload versions page to get updated reviewed_at
      await loadVersionPage(versionPage, versionOrder, selectedVersion.item_version)
      onItemChanged?.()
    } catch {
      // silently fail
    } finally {
      setReviewingHash(false)
    }
  }

  const handleToggleValReview = async () => {
    if (!selectedVersion || reviewingVal) return
    setReviewingVal(true)
    try {
      const isReviewed = selectedVersion.val_reviewed_at != null
      await setIntegrityReviewed(itemId, selectedVersion.item_version, !isReviewed, null)
      await loadVersionPage(versionPage, versionOrder, selectedVersion.item_version)
      onItemChanged?.()
    } catch {
      // silently fail
    } finally {
      setReviewingVal(false)
    }
  }

  // Spacing constants
  const sp = {
    icon: isPanel ? 'h-3 w-3' : 'h-3.5 w-3.5',
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
    const icon =
      kind === 'initial' ? <Plus className={`${sp.icon} text-green-500`} /> :
      kind === 'modified' ? <Triangle className={`${sp.icon} text-blue-500`} fill="currentColor" /> :
      kind === 'deleted' ? <X className={`${sp.icon} text-red-500`} /> :
      <Plus className={`${sp.icon} text-green-500`} /> // restored
    const label =
      kind === 'initial' ? 'Added' :
      kind === 'modified' ? 'Modified' :
      kind === 'deleted' ? 'Deleted' :
      'Restored'
    return (
      <span className={`inline-flex items-center ${sp.gap} text-xs flex-shrink-0`}>
        {icon}
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


  const pathLine = parentPath ? (
    <button
      className={cn(
        "text-muted-foreground flex items-center gap-1 text-left w-full transition-colors",
        (pathTruncated || pathExpanded) && "hover:text-foreground cursor-pointer",
        !pathTruncated && !pathExpanded && "cursor-default",
        isSheet ? 'text-sm' : 'text-xs',
      )}
      onClick={() => (pathTruncated || pathExpanded) && setPathExpanded(!pathExpanded)}
    >
      <span className="flex-shrink-0">in </span>
      <span ref={pathRef} className={pathExpanded ? 'break-all' : 'truncate'}>{parentPath}</span>
      {(pathTruncated || pathExpanded) && (
        <ChevronDown className={cn("h-3 w-3 flex-shrink-0 transition-transform", !pathExpanded && "-rotate-90")} />
      )}
    </button>
  ) : null

  const renderHeader = () => {
    const icon = itemType === 'D'
      ? <Folder className={`${isSheet ? 'h-8 w-8' : 'h-5 w-5'} text-foreground`} />
      : <File className={`${isSheet ? 'h-8 w-8' : 'h-5 w-5'} text-muted-foreground`} />

    if (isSheet) {
      return (
        <SheetHeader>
          <div className="flex items-start gap-3">
            <div className="flex-shrink-0 mt-0.5">{icon}</div>
            <div className="flex-1 min-w-0">
              <SheetTitle className="text-lg font-bold break-words">{itemName}</SheetTitle>
              {pathLine}
            </div>
          </div>
        </SheetHeader>
      )
    }

    return (
      <div className="bg-card px-3 py-2 border-b border-border">
        <div className="flex items-center gap-2">
          <div className="flex-shrink-0">{icon}</div>
          <div className="flex-1 min-w-0">
            <p className="text-base font-semibold truncate">{itemName}</p>
            {pathLine}
          </div>
          <Button variant="ghost" size="sm" className="h-6 w-6 p-0 flex-shrink-0" onClick={onClose}>
            <X className="h-3.5 w-3.5" />
          </Button>
        </div>
      </div>
    )
  }

  const renderItemProperties = () => (
    <div className={`flex items-center justify-between text-sm ${isPanel ? 'text-xs' : ''}`}>
      <div className="flex items-center gap-4">
        <span><span className="text-muted-foreground">Item:</span> <span className="font-mono font-semibold">#{itemId}</span></span>
        <span><span className="text-muted-foreground">Type:</span> {itemTypeLabel(itemType)}</span>
        {isTombstone && <Badge variant="destructive">Deleted</Badge>}
      </div>
      {itemType === 'F' && integrityState && (
        <HoverCard openDelay={300}>
          <HoverCardTrigger asChild>
            <div className="flex items-center gap-1.5 cursor-default">
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
              <span className="text-xs text-muted-foreground">Validate</span>
            </div>
          </HoverCardTrigger>
          <HoverCardContent side="bottom" align="end" className="w-56 text-xs">
            {integrityState.do_not_validate
              ? <p>Validation is <span className="font-semibold">disabled</span> for this file. Enable to include it in future validation scans.</p>
              : <p>This file will be <span className="font-semibold">validated</span> during future scans. Disable to skip validation for this file.</p>
            }
          </HoverCardContent>
        </HoverCard>
      )}
    </div>
  )

  // Render version detail content for a given version.
  // Hash timeline and review controls only render for the selected version
  // (since that data is loaded on selection). Other slides show a summary.
  const renderVersionDetail = (v: VersionEntry) => {
    const isSelected = v.item_version === selectedVersion?.item_version
    const content = (
      <>
        {/* Metadata fields — inline label: value */}
        <div className={`text-sm ${isPanel ? 'space-y-1' : 'space-y-1.5'}`}>
          <p><span className="text-muted-foreground">Scans:</span> {scanRangeLabel(v)}</p>
          <p><span className="text-muted-foreground">Modified:</span> {v.mod_date ? formatDateFull(v.mod_date) : 'N/A'}</p>
          {v.size !== null && (
            <p><span className="text-muted-foreground">Size:</span> {formatFileSize(v.size)}</p>
          )}
        </div>

        {/* Integrity (files only) — full detail only for selected version */}
        {itemType === 'F' && integrityState && isSelected && (() => {
          const suspectCount = hashHistory.filter(h => h.hash_state === 2).length
          const hasHashes = hashHistory.length > 0
          const hashPageCount = Math.ceil(hashHistory.length / HASHES_PER_PAGE)
          const hashPageStart = hashHistoryPage * HASHES_PER_PAGE
          const hashPageSlice = hashHistory.slice(hashPageStart, hashPageStart + HASHES_PER_PAGE)

          return (
            <div className={`${isPanel ? 'mt-2 pt-2' : 'mt-3 pt-3'} border-t space-y-3`}>

              {/* Hash section */}
              <div className="border border-border rounded-lg p-3 space-y-2">
                <div className="flex items-center justify-between">
                  <p className="text-sm">
                    <span className="text-muted-foreground">Hash:</span>{' '}
                    {loadingHashHistory ? (
                      <span className="text-muted-foreground">Loading...</span>
                    ) : hasHashes ? (
                      suspectCount > 0 ? (
                        <span className="font-medium">
                          Baseline &rarr; {suspectCount} Suspect{' '}
                          <AlertTriangle className="inline h-3.5 w-3.5 text-amber-500 align-text-bottom" />
                        </span>
                      ) : (
                        <span className="font-medium">Baseline</span>
                      )
                    ) : (
                      <span className="text-muted-foreground">Not hashed</span>
                    )}
                  </p>
                  {hasHashes && suspectCount > 0 && (
                    <HoverCard openDelay={300}>
                      <HoverCardTrigger asChild>
                        <span>
                          <ReviewToggle
                            size="sm"
                            reviewed={v.hash_reviewed_at != null}
                            onToggle={handleToggleHashReview}
                            disabled={reviewingHash}
                          />
                        </span>
                      </HoverCardTrigger>
                      <HoverCardContent side="bottom" align="end" className="w-56 text-xs">
                        {v.hash_reviewed_at != null
                          ? <p>Mark this suspect hash as <span className="font-semibold">unreviewed</span></p>
                          : <p>Mark this suspect hash as <span className="font-semibold">reviewed</span></p>
                        }
                      </HoverCardContent>
                    </HoverCard>
                  )}
                </div>

                {/* Hash timeline table */}
                {hasHashes && (
                  <>
                    <div className="border border-border rounded-md overflow-hidden">
                      <table className="w-full text-xs">
                        <thead>
                          <tr className="bg-muted">
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">Scan</th>
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">State</th>
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">Hash</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-border">
                          {hashPageSlice.map((h, idx) => {
                            const globalIdx = hashPageStart + idx
                            const isExpanded = expandedHashes[globalIdx] || false
                            return (
                              <tr key={h.first_scan_id} className={h.hash_state === 2 ? 'bg-amber-500/5' : ''}>
                                <td className="px-2 py-1 whitespace-nowrap text-muted-foreground">
                                  #{h.first_scan_id} ({formatScanDate(h.scan_started_at)})
                                </td>
                                <td className="px-2 py-1 whitespace-nowrap">
                                  <span className="flex items-center gap-1">
                                    {h.hash_state === 1 ? 'Baseline' : 'Suspect'}
                                    {h.hash_state === 2 && <AlertTriangle className="h-3 w-3 text-amber-500" />}
                                  </span>
                                </td>
                                <td className="px-2 py-1">
                                  <button
                                    className="font-mono text-left hover:text-foreground transition-colors flex items-center gap-1"
                                    onClick={() => setExpandedHashes(prev => ({ ...prev, [globalIdx]: !isExpanded }))}
                                  >
                                    <span className={isExpanded ? 'break-all' : ''}>
                                      {isExpanded ? h.file_hash : h.file_hash.slice(0, 12) + '\u2026'}
                                    </span>
                                    <ChevronDown className={cn("h-3 w-3 text-muted-foreground flex-shrink-0 transition-transform", !isExpanded && "-rotate-90")} />
                                  </button>
                                </td>
                              </tr>
                            )
                          })}
                        </tbody>
                      </table>
                    </div>
                    {hashPageCount > 1 && (
                      <div className="flex items-center justify-between text-xs text-muted-foreground">
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-6 text-xs px-2"
                          disabled={hashHistoryPage <= 0}
                          onClick={() => setHashHistoryPage(p => p - 1)}
                        >
                          &larr; Prev
                        </Button>
                        <span>
                          {hashPageStart + 1}&ndash;{Math.min(hashPageStart + HASHES_PER_PAGE, hashHistory.length)} of {hashHistory.length}
                        </span>
                        <Button
                          variant="outline"
                          size="sm"
                          className="h-6 text-xs px-2"
                          disabled={hashHistoryPage >= hashPageCount - 1}
                          onClick={() => setHashHistoryPage(p => p + 1)}
                        >
                          Next &rarr;
                        </Button>
                      </div>
                    )}
                  </>
                )}
              </div>

              {/* Validation section */}
              <div className="border border-border rounded-lg p-3">
                <div className="flex items-center justify-between">
                  <p className="text-sm">
                    <span className="text-muted-foreground">Validation:</span>{' '}
                    <span className="font-medium">
                      {!integrityState.has_validator && v.val_state == null
                        ? 'No Validator'
                        : valStateLabel(v.val_state)}
                    </span>
                    {v.val_state === 2 && <CircleX className="inline h-3.5 w-3.5 text-rose-500 ml-1 align-text-bottom" />}
                  </p>
                  {v.val_state === 2 && (
                    <HoverCard openDelay={300}>
                      <HoverCardTrigger asChild>
                        <span>
                          <ReviewToggle
                            size="sm"
                            reviewed={v.val_reviewed_at != null}
                            onToggle={handleToggleValReview}
                            disabled={reviewingVal}
                          />
                        </span>
                      </HoverCardTrigger>
                      <HoverCardContent side="bottom" align="end" className="w-56 text-xs">
                        {v.val_reviewed_at != null
                          ? <p>Mark this validation error as <span className="font-semibold">unreviewed</span></p>
                          : <p>Mark this validation error as <span className="font-semibold">reviewed</span></p>
                        }
                      </HoverCardContent>
                    </HoverCard>
                  )}
                </div>
                {v.val_error && v.val_error.trim() !== '' && (
                  <p className="text-xs mt-1.5 text-muted-foreground">{v.val_error}</p>
                )}
              </div>
            </div>
          )
        })()}

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
                        <Plus className={`${sp.icon} text-green-500 flex-shrink-0`} />
                        <span className="text-muted-foreground">Added :</span>
                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <X className={`${sp.icon} text-red-500 flex-shrink-0`} />
                        <span className="text-muted-foreground">Deleted :</span>
                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <Triangle className={`${sp.icon} text-blue-500 flex-shrink-0`} fill="currentColor" />
                        <span className="text-muted-foreground">Modified :</span>
                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                      </span>
                      <span className="flex items-center gap-1.5">
                        <Minus className={`${sp.icon} text-foreground flex-shrink-0`} />
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

    return content
  }

  const renderMainCard = () => {
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

    const versionCarousel = versions.length > 0 && (
      <Carousel
        opts={{ align: 'center', watchDrag: false }}
        setApi={setCarouselApi}
        className="w-full"
      >
        <div className="flex items-center gap-1">
          <CarouselPrevious className="static translate-y-0 h-7 w-7 flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <CarouselContent>
                {versions.map((v) => (
                  <CarouselItem key={v.item_version}>
                    <div className="border border-border rounded-lg overflow-hidden">
                      <div className="bg-muted px-3 py-1.5 text-center">
                        <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground">
                          Version {v.item_version} of {totalVersionCount}
                        </p>
                      </div>
                      <div className={isPanel ? 'p-2' : 'p-4'}>
                        {renderVersionDetail(v)}
                      </div>
                    </div>
                  </CarouselItem>
                ))}
              </CarouselContent>
            </div>
          <CarouselNext className="static translate-y-0 h-7 w-7 flex-shrink-0" />
        </div>
      </Carousel>
    )

    if (isSheet) {
      return (
        <Card>
          {/* Item properties */}
          <CardContent className="py-3">
            {renderItemProperties()}
          </CardContent>
          <Separator />

          {/* Version detail carousel */}
          {versionCarousel && (
            <>
              <CardContent className="py-3">
                {versionCarousel}
              </CardContent>
            </>
          )}

          {/* Version list */}
          <div className="bg-muted px-4 py-1.5 relative flex items-center justify-end">
            <p className="absolute inset-0 flex items-center justify-center text-xs font-medium uppercase tracking-wide text-muted-foreground pointer-events-none">All Versions</p>
            {sortControl}
          </div>
          <CardContent className="pt-3 pb-3">
            {versions.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">No version history</p>
            ) : (
              <>
                <div className="border border-border rounded-lg">
                  {renderVersionRows()}
                </div>
                {totalPages > 1 && (
                  <div className="mt-3 flex items-center justify-between text-xs text-muted-foreground">
                    <Button variant="outline" size="sm" className="h-7 text-xs" disabled={versionPage <= 0} onClick={() => loadVersionPage(versionPage - 1, versionOrder)}>
                      &larr; Prev
                    </Button>
                    <span>{pageStart}&ndash;{pageEnd} of {totalVersionCount}</span>
                    <Button variant="outline" size="sm" className="h-7 text-xs" disabled={versionPage >= totalPages - 1} onClick={() => loadVersionPage(versionPage + 1, versionOrder)}>
                      Next &rarr;
                    </Button>
                  </div>
                )}
              </>
            )}
          </CardContent>
        </Card>
      )
    }

    // Panel mode
    return (
      <div>
        <div className="px-3 py-2 border-b border-border">
          {renderItemProperties()}
        </div>
        {versionCarousel && (
          <div className="py-2 px-1.5">
            {versionCarousel}
          </div>
        )}
        <div className="bg-muted/50 px-3 py-1.5 relative flex items-center justify-end border-y border-border">
          <p className="absolute inset-0 flex items-center justify-center text-xs font-medium uppercase tracking-wide text-muted-foreground pointer-events-none">All Versions</p>
          {sortControl}
        </div>
        <div className="px-3 py-3">
          {versions.length === 0 ? (
            <p className="text-sm text-muted-foreground text-center py-4">No version history</p>
          ) : (
            <>
              <div className="divide-y divide-border">
                {renderVersionRows()}
              </div>
              {totalPages > 1 && (
                <div className="mt-2 flex items-center justify-between text-xs text-muted-foreground">
                  <Button variant="outline" size="sm" className="h-7 text-xs" disabled={versionPage <= 0} onClick={() => loadVersionPage(versionPage - 1, versionOrder)}>
                    &larr; Prev
                  </Button>
                  <span>{pageStart}&ndash;{pageEnd} of {totalVersionCount}</span>
                  <Button variant="outline" size="sm" className="h-7 text-xs" disabled={versionPage >= totalPages - 1} onClick={() => loadVersionPage(versionPage + 1, versionOrder)}>
                    Next &rarr;
                  </Button>
                </div>
              )}
            </>
          )}
        </div>
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

  const renderVersionRows = () => {
    return (
      <>
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
                                        <Plus className={`${sp.icon} text-green-500 flex-shrink-0`} />
                                        <span className="text-muted-foreground">Added:</span>
                                        <span className="text-muted-foreground">{(prev.add_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.delete_count !== prev.delete_count && (
                                      <div className="flex items-center gap-1.5">
                                        <X className={`${sp.icon} text-red-500 flex-shrink-0`} />
                                        <span className="text-muted-foreground">Deleted:</span>
                                        <span className="text-muted-foreground">{(prev.delete_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.modify_count !== prev.modify_count && (
                                      <div className="flex items-center gap-1.5">
                                        <Triangle className={`${sp.icon} text-blue-500 flex-shrink-0`} fill="currentColor" />
                                        <span className="text-muted-foreground">Modified:</span>
                                        <span className="text-muted-foreground">{(prev.modify_count ?? 0).toLocaleString()}</span>
                                        <span>&rarr;</span>
                                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {v.unchanged_count !== prev.unchanged_count && (
                                      <div className="flex items-center gap-1.5">
                                        <Minus className={`${sp.icon} text-foreground flex-shrink-0`} />
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
                                        <Plus className={`${sp.icon} text-green-500 flex-shrink-0`} />
                                        <span className="text-muted-foreground">Added:</span>
                                        <span className="font-medium">{(v.add_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.delete_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <X className={`${sp.icon} text-red-500 flex-shrink-0`} />
                                        <span className="text-muted-foreground">Deleted:</span>
                                        <span className="font-medium">{(v.delete_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.modify_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <Triangle className={`${sp.icon} text-blue-500 flex-shrink-0`} fill="currentColor" />
                                        <span className="text-muted-foreground">Modified:</span>
                                        <span className="font-medium">{(v.modify_count ?? 0).toLocaleString()}</span>
                                      </div>
                                    )}
                                    {(v.unchanged_count ?? 0) > 0 && (
                                      <div className="flex items-center gap-1.5">
                                        <Minus className={`${sp.icon} text-foreground flex-shrink-0`} />
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
      </>
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
        <div className={isSheet ? 'space-y-4 mt-4' : 'divide-y divide-border border-b border-border'}>
          {renderMainCard()}
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
