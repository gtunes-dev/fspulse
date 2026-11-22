import { useState, useEffect, useCallback, useRef, type ReactNode } from 'react'
import { format } from 'date-fns'
import {
  File, Folder, FolderTree,
  HardDrive, AlertTriangle, CircleX, ChevronDown, ChevronLeft, ChevronRight, Eye, X,
  ShieldCheck, ShieldOff, Plus, Triangle,
  Minus,
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
  ReferenceArea,
  ReferenceLine,
} from 'recharts'
import { Separator } from '@/components/ui/separator'
import {
  HoverCard,
  HoverCardTrigger,
  HoverCardContent,
} from '@/components/ui/hover-card'
import { formatDateFull } from '@/lib/dateUtils'
import { DAY, HOUR, formatMoment, nowSeconds } from '@/lib/timeRange'
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
  /** Show the item as of this time (Unix seconds); null for now. */
  at: number | null
  /**
   * Start of the selected range (Unix seconds), when the item is being
   * looked at over a range: the pane then says what happened to the
   * item inside it and marks the versions that fall in it.
   */
  from?: number | null
  /** The root the item is in. With a range, a folder then says what changed beneath it. */
  rootId?: number
  /** Changes when the surrounding view is read again, so the pane reads again too. */
  refreshToken?: number
  /** Given when the surrounding view can be moved to a time: a version then offers to show the tree as it was. */
  onShowMoment?: (time: number) => void
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
  first_seen_at: number
  last_changed_at: number
  change_count: number
  source: number
  is_added: boolean
  is_deleted: boolean
  /** Written because of the ignore patterns: stopped (with is_deleted) or resumed (with is_added). */
  is_ignored: boolean
  access: number
  mod_date: number | null
  size: number | null
  hash_state: number | null
  file_hash: string | null
  val_state: number | null
  val_error: string | null
  /** When this version was validated. */
  val_at: number | null
  /** When this version's latest hash was last confirmed. */
  hash_last_seen_at: number | null
  val_reviewed_at: number | null
  hash_reviewed_at: number | null
}

/** What happened to the item inside the selected range, from `/api/items/:id/in-range`. */
interface InRange {
  /** The range as the server resolved it. */
  range: { from: number | null; to: number; to_is_now: boolean }
  changes: number
  happened: { added: boolean; modified: boolean; deleted: boolean }
  tracking_stopped: boolean
  tracking_resumed: boolean
}

interface HashEntry {
  first_seen_at: number
  last_seen_at: number
  file_hash: string
  hash_state: number
}

interface VersionPageResponse {
  versions: VersionEntry[]
}

/** The item's size from one moment until the next point, from `/api/items/:id/size-history`. */
interface SizePoint {
  time: number
  /** Null while a file is deleted or not tracked. */
  size: number | null
}

/** The version in effect at the time shown, from `/api/items/:id/version-at`. */
interface VersionAt {
  item_version: number
  is_deleted: boolean
  is_ignored: boolean
}

/** What happened beneath a folder inside the selected range, from `/api/timeline/summary`. */
interface ChangesBeneath {
  added: number
  modified: number
  deleted: number
  changed: number
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

type ChangeKind = 'initial' | 'modified' | 'deleted' | 'restored' | 'ignored' | 'resumed'

// ---- Constants ----

const VERSIONS_PER_PAGE = 10
const HASHES_PER_PAGE = 10
/** How often a pane showing the present asks whether its item has changed. */
const LIVE_POLL_MS = 10_000

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
  // Ignoring and resuming are not deletions or additions.
  if (v.is_ignored && v.is_deleted) return 'ignored'
  if (v.is_ignored && v.is_added) return 'resumed'
  if (v.is_added) return 'initial'
  if (v.is_deleted && prev && !prev.is_deleted) return 'deleted'
  if (v.is_deleted) return 'deleted'
  if (!v.is_deleted && prev?.is_deleted) return 'restored'
  return 'modified'
}

const clock = (seconds: number): string => format(new Date(seconds * 1000), 'HH:mm')

/**
 * When a version was recorded. One change has a moment. Several changes
 * in the same hour are one version, so they have the hour, how many
 * there were and when the last one was.
 */
function recordedLabel(v: VersionEntry): string {
  if (v.change_count <= 1) return formatMoment(v.first_seen_at)
  const start = v.first_seen_at - (v.first_seen_at % HOUR)
  return `${formatMoment(start)} to ${clock(start + HOUR)}, changed ${v.change_count} times, last at ${clock(v.last_changed_at)}`
}

/** The same in few words, for a row of the list. The card has the rest. */
function recordedShort(v: VersionEntry): string {
  if (v.change_count <= 1) return formatMoment(v.first_seen_at)
  const start = v.first_seen_at - (v.first_seen_at % HOUR)
  return `${formatMoment(start)} to ${clock(start + HOUR)} \u00b7 ${v.change_count} changes`
}

/**
 * How a version came to be recorded, where that matters to reading its
 * time. A change seen as it happened needs no remark. One found by a
 * check happened at some earlier time that was not seen.
 */
function sourceNote(v: VersionEntry): string | null {
  switch (v.source) {
    case 1:
      return 'Found by a check. The change itself happened some time before.'
    case 3:
      return 'Noticed while the file was being checked for integrity.'
    default:
      return null
  }
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
  at,
  from = null,
  rootId,
  refreshToken,
  onShowMoment,
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

  // What happened to the item inside the selected range
  const [inRange, setInRange] = useState<InRange | null>(null)
  // For a folder, what happened beneath it inside the selected range
  const [changesBeneath, setChangesBeneath] = useState<ChangesBeneath | null>(null)

  // Size over everything recorded; null until read
  const [sizeHistory, setSizeHistory] = useState<SizePoint[] | null>(null)

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

  // The version shown in the card — stored as full data, not derived from page
  const [selectedVersion, setSelectedVersion] = useState<VersionEntry | null>(null)
  // The version in effect at the time the pane shows the item as of.
  const [versionAtEnd, setVersionAtEnd] = useState<VersionAt | null>(null)

  // Reading again. The server's stamp for the item changes when anything
  // recorded about it does; `reloadKey` is bumped when a new one is seen.
  const [reloadKey, setReloadKey] = useState(0)
  const stamp = useRef<string | null>(null)
  const refreshKey = `${reloadKey}:${refreshToken ?? 0}`
  // Which item and time the versions were last read for, to tell a
  // refresh from a new subject.
  const loadedFor = useRef('')
  // The version to stay on through a refresh: the one being looked at,
  // unless that is the one in effect, which is followed to its successor.
  const keepVersion = useRef<number | null>(null)

  const isPanel = mode === 'panel'
  const isSheet = mode === 'sheet'
  const lastSlash = itemPath.lastIndexOf('/')
  const itemName = lastSlash >= 0 ? itemPath.slice(lastSlash + 1) : itemPath
  const parentPath = lastSlash > 0 ? itemPath.slice(0, lastSlash) : ''

  // For sheet mode, skip data loading when not open
  const shouldLoad = isPanel || open === true

  // Reset state when switching items
  useEffect(() => {
    setPathExpanded(false)
    setPathTruncated(false)
    setSelectedVersion(null)
    setVersionAtEnd(null)
    setSizeHistory(null)
    stamp.current = null
  }, [itemId])

  useEffect(() => {
    keepVersion.current =
      selectedVersion && selectedVersion.item_version !== versionAtEnd?.item_version
        ? selectedVersion.item_version
        : null
  }, [selectedVersion, versionAtEnd])

  // Detect if path text is truncated
  useEffect(() => {
    if (pathExpanded) return
    const el = pathRef.current
    if (el) setPathTruncated(el.scrollWidth > el.clientWidth)
  }, [parentPath, pathExpanded])

  // ---- Data loading ----

  // Load a page of versions with one extra record for predecessor diffs.
  // selectVersion: jump to a specific version after loading.
  // autoSelect: always select the first version (used on initial load to avoid stale closure).
  const loadVersionPage = useCallback(async (
    page: number,
    order: 'asc' | 'desc',
    selectVersion?: number,
    autoSelect?: boolean,
  ): Promise<VersionEntry | null> => {
    if (!shouldLoad) return null
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
          return found ?? null
        } else if (autoSelect && displayVersions.length > 0) {
          setSelectedVersion(displayVersions[0])
          return displayVersions[0]
        }
      }
    } catch (error) {
      console.error('Error loading versions:', error)
    }
    return null
  }, [shouldLoad, itemId])

  // Read the versions: find the version in effect at the time shown,
  // load the page of the list that holds it and show it in the card.
  // Reading again for the same item and time (a refresh) does not blank
  // the pane and stays on the version being looked at.
  useEffect(() => {
    if (!shouldLoad) return
    const subject = `${itemId}:${at}`
    const isRefresh = loadedFor.current === subject
    loadedFor.current = subject
    if (!isRefresh) setLoadingVersions(true)
    let current = true

    async function load() {
      let inEffect: VersionAt | null = null
      let total = 0
      try {
        const [atResponse, countResponse] = await Promise.all([
          fetch(`/api/items/${itemId}/version-at${at !== null ? `?at=${at}` : ''}`),
          fetch(`/api/items/${itemId}/version-count`),
        ])
        if (countResponse.ok) {
          const count = await countResponse.json()
          total = count.total
          stamp.current = count.stamp
        }
        // 404: nothing had been recorded about the item at that time.
        if (atResponse.ok) inEffect = await atResponse.json()
      } catch (error) {
        console.error('Error resolving version at time:', error)
      }
      if (!current) return
      setTotalVersionCount(total)
      setVersionAtEnd(inEffect)

      const kept = isRefresh && keepVersion.current !== null && keepVersion.current <= total ? keepVersion.current : null
      const show = kept ?? inEffect?.item_version ?? null
      if (show === null) {
        await loadVersionPage(0, versionOrder, undefined, true)
        return
      }
      const position = versionOrder === 'asc' ? show - 1 : total - show
      await loadVersionPage(Math.floor(position / VERSIONS_PER_PAGE), versionOrder, show)
    }

    load().finally(() => {
      if (current) setLoadingVersions(false)
    })
    return () => {
      current = false
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shouldLoad, itemId, at, refreshKey]) // intentionally not including versionOrder or loadVersionPage

  // While the present is shown, ask now and then whether anything
  // recorded about the item has changed, and read again if it has.
  useEffect(() => {
    if (!shouldLoad || at !== null) return
    const timer = window.setInterval(async () => {
      if (document.hidden) return
      try {
        const response = await fetch(`/api/items/${itemId}/version-count`)
        if (!response.ok) return
        const count = await response.json()
        if (stamp.current !== null && count.stamp !== stamp.current) {
          stamp.current = count.stamp
          setReloadKey(key => key + 1)
        }
      } catch {
        // The next tick asks again.
      }
    }, LIVE_POLL_MS)
    return () => window.clearInterval(timer)
  }, [shouldLoad, itemId, at])

  // What happened inside the selected range
  useEffect(() => {
    if (!shouldLoad || from === null) {
      setInRange(null)
      return
    }
    let current = true
    const params = new URLSearchParams({ from: from.toString() })
    if (at !== null) params.set('to', at.toString())
    fetch(`/api/items/${itemId}/in-range?${params}`)
      .then(response => (response.ok ? response.json() : null))
      .then((data: InRange | null) => {
        // A start that is not before the end is not a range.
        if (current) setInRange(data && data.range.from !== null ? data : null)
      })
      .catch(() => {
        if (current) setInRange(null)
      })
    return () => {
      current = false
    }
  }, [shouldLoad, itemId, from, at, refreshKey])

  // For a folder: what happened beneath it inside the selected range
  useEffect(() => {
    if (!shouldLoad || itemType !== 'D' || from === null || rootId === undefined) {
      setChangesBeneath(null)
      return
    }
    let current = true
    const params = new URLSearchParams({ root_id: rootId.toString(), path: itemPath, from: from.toString() })
    if (at !== null) params.set('to', at.toString())
    fetch(`/api/timeline/summary?${params}`)
      .then(response => (response.ok ? response.json() : null))
      .then((data: { range: InRange['range']; counts: ChangesBeneath } | null) => {
        if (current) setChangesBeneath(data && data.range.from !== null ? data.counts : null)
      })
      .catch(() => {
        if (current) setChangesBeneath(null)
      })
    return () => {
      current = false
    }
  }, [shouldLoad, itemId, itemType, itemPath, rootId, from, at, refreshKey])

  /** Whether a version began inside the selected range. */
  const isInRange = (v: VersionEntry): boolean => {
    if (!inRange || inRange.range.from === null) return false
    return v.first_seen_at >= inRange.range.from && (inRange.range.to_is_now || v.first_seen_at < inRange.range.to)
  }

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

  /** Show the version before or after the one in the card, loading the page of the list it is on if need be. */
  const stepVersion = (by: -1 | 1) => {
    if (!selectedVersion) return
    const target = selectedVersion.item_version + by
    if (target < 1 || target > totalVersionCount) return
    const loaded = versions.find(v => v.item_version === target)
    if (loaded) {
      setSelectedVersion(loaded)
      return
    }
    const position = versionOrder === 'asc' ? target - 1 : totalVersionCount - target
    loadVersionPage(Math.floor(position / VERSIONS_PER_PAGE), versionOrder, target)
  }

  // ---- Size history ----

  useEffect(() => {
    if (!shouldLoad) return
    let current = true
    fetch(`/api/items/${itemId}/size-history`)
      .then(response => (response.ok ? response.json() : null))
      .then((data: { points: SizePoint[] } | null) => {
        if (current) setSizeHistory(data?.points ?? [])
      })
      .catch(error => {
        console.error('Error loading size history:', error)
        if (current) setSizeHistory([])
      })
    return () => {
      current = false
    }
  }, [shouldLoad, itemId, refreshKey])

  // ---- Children counts ----

  useEffect(() => {
    async function loadChildrenCounts() {
      if (!shouldLoad || itemType !== 'D' || isTombstone) {
        setChildrenCounts(null)
        return
      }
      setLoadingChildrenCounts(true)
      try {
        const response = await fetch(`/api/items/${itemId}/children-counts${at !== null ? `?at=${at}` : ''}`)
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
  }, [shouldLoad, itemId, itemType, isTombstone, at, refreshKey])

  // ---- Integrity state (files only) ----

  const loadIntegrity = useCallback(async () => {
    if (!shouldLoad || itemType !== 'F') {
      setIntegrityState(null)
      return
    }
    try {
      const response = await fetch(`/api/items/${itemId}/integrity-state${at !== null ? `?at=${at}` : ''}`)
      if (response.ok) {
        setIntegrityState(await response.json())
      } else {
        setIntegrityState(null)
      }
    } catch {
      setIntegrityState(null)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shouldLoad, itemId, itemType, at, refreshKey])

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
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [shouldLoad, itemId, selectedVersion, itemType, refreshKey])

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
      kind === 'ignored' || kind === 'resumed' ? <Minus className={`${sp.icon} text-muted-foreground`} /> :
      <Plus className={`${sp.icon} text-green-500`} /> // restored
    const label =
      kind === 'initial' ? 'Added' :
      kind === 'modified' ? 'Modified' :
      kind === 'deleted' ? 'Deleted' :
      kind === 'ignored' ? 'Tracking stopped (ignored)' :
      kind === 'resumed' ? 'Tracking resumed' :
      'Restored'
    return (
      <span className={`inline-flex items-center ${sp.gap} text-xs flex-shrink-0`}>
        {icon}
        <span>{label}</span>
      </span>
    )
  }

  // What the item was at the time shown: there, deleted, or no longer tracked.
  const endState: 'deleted' | 'ignored' | null = versionAtEnd
    ? versionAtEnd.is_deleted ? (versionAtEnd.is_ignored ? 'ignored' : 'deleted') : null
    : isTombstone ? 'deleted' : null

  // The time shown, as the server reads it: the present, or the start
  // of the hour asked for.
  const shownAt = at === null ? null : at - (at % HOUR)

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
        {endState === 'deleted' && <Badge variant="destructive">Deleted</Badge>}
        {endState === 'ignored' && <Badge variant="secondary">Not tracked (ignored)</Badge>}
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
              ? <p>Validation is <span className="font-semibold">disabled</span> for this file. Enable to include it in future integrity checks.</p>
              : <p>This file will be <span className="font-semibold">validated</span> during future integrity checks. Disable to skip validation for this file.</p>
            }
          </HoverCardContent>
        </HoverCard>
      )}
    </div>
  )

  // The mark that ties the "In this range" line to the versions in the list.
  const rangeMark = <span className="inline-block w-0.5 h-3 rounded-full bg-primary flex-shrink-0" aria-hidden />

  const renderInRange = () => {
    if (!inRange) return null
    const happened = [
      inRange.happened.added && 'added',
      inRange.happened.modified && 'modified',
      inRange.happened.deleted && 'deleted',
      inRange.tracking_stopped && 'tracking stopped',
      inRange.tracking_resumed && 'tracking resumed',
    ].filter(Boolean)
    return (
      <p className={cn('flex items-center gap-1.5', isPanel ? 'text-xs mt-1.5' : 'text-sm mt-2')}>
        {happened.length === 0 ? (
          <span className="text-muted-foreground">Unchanged in the selected range</span>
        ) : (
          <>
            {rangeMark}
            <span className="text-muted-foreground">In the selected range:</span>
            <span className="font-medium">{happened.join(', ')}</span>
            {inRange.changes > 0 && (
              <span className="text-muted-foreground">
                &middot; {inRange.changes.toLocaleString()} {inRange.changes === 1 ? 'change' : 'changes'}
              </span>
            )}
          </>
        )}
      </p>
    )
  }

  // The card's content for the version it shows: what it was, when it
  // was recorded, and what differs from the version before it.
  const renderVersionDetail = (v: VersionEntry) => {
    const prev = predecessors.get(v.item_version) ?? null
    const kind = classifyChange(v, prev)
    // What a value was before, where both versions are of the item present.
    const comparable = prev !== null && !prev.is_deleted && !v.is_deleted
    const was = (changed: boolean, before: string) =>
      comparable && changed ? <span className="text-muted-foreground"> &middot; was {before}</span> : null
    const note = sourceNote(v)
    const alreadyShown = v.item_version === versionAtEnd?.item_version
    const content = (
      <>
        <div className={`text-sm ${isPanel ? 'space-y-1' : 'space-y-1.5'}`}>
          <div>{getChangeIndicator(kind)}</div>
          <p><span className="text-muted-foreground">Recorded:</span> {recordedLabel(v)}</p>
          {note && <p className="text-xs text-muted-foreground">{note}</p>}
          {v.mod_date != null && (
            <p>
              <span className="text-muted-foreground">File date:</span> {formatDateFull(v.mod_date)}
              {was(prev?.mod_date !== v.mod_date, prev?.mod_date != null ? formatDateFull(prev.mod_date) : 'not set')}
            </p>
          )}
          {v.size !== null && (
            <p>
              <span className="text-muted-foreground">Size:</span> {formatFileSize(v.size)}
              {was(prev?.size !== v.size, formatFileSize(prev?.size ?? null))}
            </p>
          )}
          {(v.access !== 0 || (comparable && prev.access !== v.access)) && (
            <p>
              <span className="text-muted-foreground">Access:</span> {accessLabel(v.access)}
              {was(prev?.access !== v.access, accessLabel(prev?.access ?? 0))}
            </p>
          )}
        </div>

        {/* Integrity (files only). A deleted version has no content to check. */}
        {itemType === 'F' && integrityState && !v.is_deleted && (() => {
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
                      <span className="text-muted-foreground">Not hashed yet</span>
                    )}
                    {hasHashes && v.hash_last_seen_at != null && (
                      <span className="text-muted-foreground"> &middot; last confirmed {formatMoment(v.hash_last_seen_at)}</span>
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
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">First seen</th>
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">Last confirmed</th>
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">State</th>
                            <th className="text-left px-2 py-1 font-medium uppercase tracking-wide text-muted-foreground">Hash</th>
                          </tr>
                        </thead>
                        <tbody className="divide-y divide-border">
                          {hashPageSlice.map((h, idx) => {
                            const globalIdx = hashPageStart + idx
                            const isExpanded = expandedHashes[globalIdx] || false
                            return (
                              <tr key={h.first_seen_at} className={h.hash_state === 2 ? 'bg-amber-500/5' : ''}>
                                <td className="px-2 py-1 whitespace-nowrap text-muted-foreground">
                                  {formatMoment(h.first_seen_at)}
                                </td>
                                <td className="px-2 py-1 whitespace-nowrap text-muted-foreground">
                                  {formatMoment(h.last_seen_at)}
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
                    {v.val_state != null && v.val_at != null && (
                      <span className="text-muted-foreground"> &middot; checked {formatMoment(v.val_at)}</span>
                    )}
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

        {/* Move the surrounding view to this version's time */}
        {onShowMoment && (
          <div className={isPanel ? 'mt-2' : 'mt-3'}>
            <Button
              variant="outline"
              size="sm"
              className="h-6 px-2 text-xs gap-1"
              disabled={alreadyShown}
              title={alreadyShown
                ? 'This is the version at the time the tree is showing'
                : 'Show everything as it was at the end of the hour this version was recorded in'}
              onClick={() => onShowMoment(v.first_seen_at)}
            >
              <FolderTree className="h-3 w-3" />
              Show the tree at this time
            </Button>
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

    // One version at a time, with the way to the one before and the one
    // after. Older is to the left, as on the time control.
    const arrow = (by: -1 | 1) => {
      const target = (selectedVersion?.item_version ?? 0) + by
      const label = by === -1 ? 'Older version' : 'Newer version'
      return (
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6 flex-shrink-0"
          disabled={!selectedVersion || target < 1 || target > totalVersionCount}
          onClick={() => stepVersion(by)}
          aria-label={label}
          title={label}
        >
          {by === -1 ? <ChevronLeft className="h-4 w-4" /> : <ChevronRight className="h-4 w-4" />}
        </Button>
      )
    }
    const versionCard = selectedVersion && (
      <div className="border border-border rounded-lg overflow-hidden">
        <div className="bg-muted px-1 py-1 flex items-center gap-1">
          {arrow(-1)}
          <p className="flex-1 min-w-0 truncate text-center text-xs font-medium uppercase tracking-wide text-muted-foreground">
            Version {selectedVersion.item_version} of {totalVersionCount}
            {isInRange(selectedVersion) && <span className="text-primary"> &middot; in the selected range</span>}
          </p>
          {arrow(1)}
        </div>
        <div className={isPanel ? 'p-2' : 'p-4'}>
          {renderVersionDetail(selectedVersion)}
        </div>
      </div>
    )

    if (isSheet) {
      return (
        <Card>
          {/* Item properties */}
          <CardContent className="py-3">
            {renderItemProperties()}
            {renderInRange()}
          </CardContent>
          <Separator />

          {/* The version being looked at */}
          {versionCard && (
            <CardContent className="py-3">
              {versionCard}
            </CardContent>
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
          {renderInRange()}
        </div>
        {versionCard && (
          <div className="py-2 px-3">
            {versionCard}
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

  // The size at the time shown: the last point before it.
  const sizeAtShownTime = (): number | null => {
    let size: number | null = null
    for (const point of sizeHistory ?? []) {
      if (shownAt !== null && point.time >= shownAt) break
      size = point.size
    }
    return size
  }

  // Everything recorded about the item's size, with the selected range
  // (or, for a point in the past, the time shown) marked on it.
  const renderSizeHistory = () => {
    const points = sizeHistory ?? []
    const now = nowSeconds()
    const start = points.length > 0 ? points[0].time : now
    // Room after a single recent point, so the line has somewhere to go.
    const end = Math.max(now, start + HOUR)
    // The last size holds until the end.
    const data = points.length > 0 ? [...points, { time: end, size: points[points.length - 1].size }] : []

    const shownEnd = Math.min(shownAt ?? end, end)
    const rangeStart = from !== null ? Math.max(from - (from % HOUR), start) : null
    const shaded = rangeStart !== null && rangeStart < shownEnd
    const marked = from === null && shownAt !== null && shownAt > start && shownAt < end

    const span = end - start
    const tickFormat = span <= 2 * DAY ? 'MMM d, HH:mm' : span <= 366 * DAY ? 'MMM d' : 'MMM yyyy'
    const tickStyle = { fill: 'var(--muted-foreground)', fontSize: isPanel ? 10 : 12 }

    const chart = (
      <ChartContainer
        config={{ size: { label: 'Size', color: 'hsl(271 81% 56%)' } }}
        className={`aspect-auto ${sp.chartHeight}`}
      >
        <LineChart data={data} margin={{ top: 8, right: 12, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
          <XAxis
            dataKey="time"
            type="number"
            domain={[start, end]}
            tickCount={isPanel ? 4 : 6}
            tick={tickStyle}
            tickFormatter={(value) => format(new Date((value as number) * 1000), tickFormat)}
          />
          <YAxis
            tick={tickStyle}
            tickFormatter={(value) => {
              const units = ['B', 'KB', 'MB', 'GB', 'TB']
              let i = 0; let s = value as number
              while (s >= 1000 && i < units.length - 1) { s /= 1000; i++ }
              return `${s.toFixed(s < 10 && i > 0 ? 1 : 0)} ${units[i]}`
            }}
            width={isPanel ? 50 : 60}
          />
          {shaded && (
            <ReferenceArea x1={rangeStart} x2={shownEnd} fill="var(--primary)" fillOpacity={0.12} stroke="none" />
          )}
          {marked && <ReferenceLine x={shownAt} stroke="var(--primary)" strokeDasharray="3 3" />}
          <ChartTooltip
            content={
              <ChartTooltipContent
                labelFormatter={(_label, payload) => {
                  const time = payload?.[0]?.payload?.time
                  return typeof time === 'number' ? formatMoment(time) : ''
                }}
              />
            }
            formatter={(v) => formatFileSize(v as number)}
          />
          <Line
            type="stepAfter"
            dataKey="size"
            stroke="var(--color-size)"
            strokeWidth={2}
            dot={false}
            connectNulls={false}
            isAnimationActive={false}
            name="Size"
          />
        </LineChart>
      </ChartContainer>
    )

    const blank = `flex items-center justify-center ${isPanel ? 'h-32' : 'h-64'} text-sm text-muted-foreground`
    return (
      <Section mode={mode} title="Size History">
        {sizeHistory === null ? (
          <div className={blank}>Loading...</div>
        ) : points.length === 0 ? (
          <div className={blank}>No size history</div>
        ) : (
          <>
            {chart}
            <p className="mt-1 text-xs text-muted-foreground">
              {itemType === 'D' ? 'The total size of the files beneath this folder, over everything recorded.' : 'Over everything recorded.'}
              {shaded && ' Shaded: the selected range.'}
              {marked && ' Dashed line: the time shown.'}
            </p>
          </>
        )}
      </Section>
    )
  }

  // What a folder holds at the time shown and, with a range, what
  // happened beneath it. The versions above are only the folder's own.
  const renderFolderContents = () => {
    if (itemType !== 'D') return null
    const size = sizeAtShownTime()
    const iconSize = isPanel ? 'h-3 w-3' : 'h-4 w-4'
    return (
      <Section mode={mode} title="Contents">
        {endState !== null ? (
          <p className="text-sm text-muted-foreground">
            {endState === 'deleted' ? 'The folder was not there at the time shown.' : 'The folder was not tracked at the time shown.'}
          </p>
        ) : !childrenCounts ? (
          <p className="text-sm text-muted-foreground">{loadingChildrenCounts ? 'Loading...' : 'Empty folder'}</p>
        ) : (
          <div className={`flex flex-wrap items-center ${isPanel ? 'gap-x-4' : 'gap-x-6'} gap-y-1 text-sm`}>
            <span className="flex items-center gap-1">
              <Folder className={`${iconSize} text-muted-foreground`} />
              <span className="font-medium">{childrenCounts.directory_count.toLocaleString()}</span>
              <span className="text-muted-foreground">{childrenCounts.directory_count === 1 ? 'folder' : 'folders'}</span>
            </span>
            <span className="flex items-center gap-1">
              <File className={`${iconSize} text-muted-foreground`} />
              <span className="font-medium">{childrenCounts.file_count.toLocaleString()}</span>
              <span className="text-muted-foreground">{childrenCounts.file_count === 1 ? 'file' : 'files'}</span>
            </span>
            {size !== null && (
              <span className="flex items-center gap-1">
                <HardDrive className={`${iconSize} text-muted-foreground`} />
                <span className="font-medium">{formatFileSize(size)}</span>
              </span>
            )}
          </div>
        )}
        <p className="mt-1 text-xs text-muted-foreground">Everything beneath this folder, at any depth.</p>
        {changesBeneath && (
          <p className={cn('flex items-center gap-1.5', isPanel ? 'text-xs mt-1.5' : 'text-sm mt-2')}>
            {changesBeneath.changed === 0 ? (
              <span className="text-muted-foreground">Nothing beneath it changed in the selected range</span>
            ) : (
              <>
                {rangeMark}
                <span className="text-muted-foreground">In the selected range, beneath it:</span>
                <span className="font-medium">
                  {changesBeneath.added.toLocaleString()} added, {changesBeneath.modified.toLocaleString()} modified, {changesBeneath.deleted.toLocaleString()} deleted
                </span>
              </>
            )}
          </p>
        )}
      </Section>
    )
  }

  // The list is for finding a version and choosing it; the card shows it.
  const renderVersionRows = () => (
    <>
      {versions.map((v, idx) => {
        const kind = classifyChange(v, predecessors.get(v.item_version) ?? null)
        const isSelected = selectedVersion?.item_version === v.item_version
        return (
          <div key={v.item_version}>
            <button
              type="button"
              className={cn(
                sp.padX,
                `flex w-full items-center text-left ${sp.gap} transition-colors`,
                isSelected ? 'bg-accent/50' : 'hover:bg-accent/20'
              )}
              aria-current={isSelected}
              onClick={() => setSelectedVersion(v)}
            >
              {/* The same width with and without the mark, so the rows line up. */}
              <span className="flex w-0.5 flex-shrink-0 justify-center">{isInRange(v) && rangeMark}</span>
              <span className="text-xs font-medium shrink-0">v{v.item_version}</span>
              {getChangeIndicator(kind)}
              {v.hash_state === 2 && <AlertTriangle className={`${sp.iconLg} text-amber-500 flex-shrink-0`} />}
              {v.val_state === 2 && <CircleX className={`${sp.iconLg} text-rose-500 flex-shrink-0`} />}
              <span className="text-xs text-muted-foreground truncate flex-1">{recordedShort(v)}</span>
              {isSelected && <Eye className={`${sp.iconLg} text-primary flex-shrink-0`} />}
            </button>
            {isSheet && idx < versions.length - 1 && <Separator />}
          </div>
        )
      })}
    </>
  )

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
          {renderFolderContents()}
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
