import { useState, useEffect, useCallback, useRef, Fragment } from 'react'
import { useSearchParams } from 'react-router-dom'
import {
  ChevronDown,
  CircleHelp,
  CircleCheckBig,
  ShieldCheck,
  ShieldOff,
} from 'lucide-react'
import { ReviewToggle } from '@/components/shared/ReviewToggle'
import {
  HoverCard,
  HoverCardTrigger,
  HoverCardContent,
} from '@/components/ui/hover-card'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import { SearchFilter } from '@/components/shared/SearchFilter'
import { RootCard } from '@/components/shared/RootCard'
import { ItemDetail } from '@/components/shared/ItemDetail'
import {
  fetchIntegrityCount,
  fetchIntegrityItems,
  fetchIntegrityVersions,
  setIntegrityReviewed,
  setDoNotValidate,
  fetchQuery,
} from '@/lib/api'
import type {
  IntegrityFilterParams,
  IntegrityItemSummary,
  IntegrityVersion,
  IntegrityVersionsResponse,
} from '@/lib/api'
import { cn } from '@/lib/utils'
import { formatTimeAgo } from '@/lib/dateUtils'
import { useTaskContext } from '@/contexts/TaskContext'

interface Root {
  root_id: number
  root_path: string
}

const ITEMS_PER_PAGE = 50
const VERSIONS_PER_EXPAND = 5

const FILE_TYPE_OPTIONS: { label: string; value: string }[] = [
  { label: 'All file types', value: 'all' },
  { label: 'Image files', value: 'jpg,jpeg,png,gif,bmp,tiff' },
  { label: 'PDF files', value: 'pdf' },
  { label: 'Audio files', value: 'flac' },
]

function parentFolder(path: string): string {
  const parts = path.split('/')
  if (parts.length >= 2) return parts[parts.length - 2]
  return ''
}

// ---------------------------------------------------------------------------
// Compact count display: ◌3 ✓1
// ---------------------------------------------------------------------------

function CountPair({ unreviewed, reviewed }: { unreviewed: number; reviewed: number }) {
  if (unreviewed + reviewed === 0) {
    return (
      <span className="inline-grid grid-cols-[18px_1rem_18px_1rem] items-center">
        <span className="col-span-4 text-center text-muted-foreground">—</span>
      </span>
    )
  }

  return (
    <span className="inline-grid grid-cols-[18px_1rem_18px_1rem] items-center gap-x-0.5">
      <CircleHelp className="h-[18px] w-[18px] text-muted-foreground" />
      <span className="tabular-nums">{unreviewed}</span>
      <CircleCheckBig className="h-[18px] w-[18px] text-muted-foreground" />
      <span className="tabular-nums text-muted-foreground">{reviewed}</span>
    </span>
  )
}

// ---------------------------------------------------------------------------
// Page
// ---------------------------------------------------------------------------

export function IntegrityPage() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [searchParams, setSearchParams] = useSearchParams()

  const [selectedRootId, setSelectedRootId] = useState<string>(searchParams.get('root_id') || '')
  const [issueType, setIssueType] = useState<string>(searchParams.get('issue_type') || 'all')
  const [fileType, setFileType] = useState<string>(searchParams.get('file_type') || 'all')
  const [status, setStatus] = useState<string>(searchParams.get('status') || 'unreviewed')
  const [pathSearch, setPathSearch] = useState<string>(searchParams.get('q') || '')
  const [currentPage, setCurrentPage] = useState(parseInt(searchParams.get('page') || '1') || 1)

  const [roots, setRoots] = useState<Root[]>([])
  const [items, setItems] = useState<IntegrityItemSummary[]>([])
  const [total, setTotal] = useState(0)
  const [initialLoading, setInitialLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const [detailItemId, setDetailItemId] = useState<number | null>(null)
  const [detailItemPath, setDetailItemPath] = useState<string>('')
  const [detailScanId, setDetailScanId] = useState<number | null>(null)
  const [detailOpen, setDetailOpen] = useState(false)

  // Expanded items: item_id -> version data
  const [expandedData, setExpandedData] = useState<Map<number, IntegrityVersionsResponse>>(new Map())
  const [pendingOps, setPendingOps] = useState<Set<string>>(new Set())

  const isInitialLoad = useRef(true)
  const lastFilterKeyRef = useRef<string>('')

  // Build the current filter params (shared across all API calls)
  const buildFilter = useCallback((): IntegrityFilterParams | null => {
    if (!selectedRootId) return null
    const rootId = parseInt(selectedRootId)
    if (isNaN(rootId)) return null
    return {
      root_id: rootId,
      issue_type: issueType === 'all' ? undefined : issueType,
      extensions: fileType === 'all' ? undefined : fileType,
      status,
      path_search: pathSearch || undefined,
    }
  }, [selectedRootId, issueType, fileType, status, pathSearch])

  // --- Data fetching ---

  useEffect(() => {
    fetchQuery('roots', {
      columns: [
        { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
        { name: 'root_path', visible: true, sort_direction: 'asc', position: 1 },
      ],
      filters: [],
      limit: 1000,
      offset: 0,
    })
      .then((response) => {
        const loaded: Root[] = response.rows.map((row) => ({
          root_id: parseInt(row[0]),
          root_path: row[1],
        }))
        setRoots(loaded)
        setSelectedRootId(prev => prev || (loaded.length > 0 ? String(loaded[0].root_id) : prev))
      })
      .catch(() => setError('Failed to load roots'))
  }, [])

  const fetchCount = useCallback(async (filter: IntegrityFilterParams) => {
    const result = await fetchIntegrityCount(filter)
    setTotal(result.total)
  }, [])

  const fetchItems = useCallback(async (filter: IntegrityFilterParams) => {
    const result = await fetchIntegrityItems({
      ...filter,
      offset: (currentPage - 1) * ITEMS_PER_PAGE,
      limit: ITEMS_PER_PAGE,
    })
    setItems(result)
  }, [currentPage])

  // Fetch count + items when filters change (shows loading state, collapses expanded)
  const fetchFilterData = useCallback(async () => {
    const filter = buildFilter()
    if (!filter) return

    setInitialLoading(true)
    setError(null)
    setExpandedData(new Map())
    try {
      await Promise.all([fetchCount(filter), fetchItems(filter)])
    } catch {
      setError('Failed to load integrity data')
    } finally {
      setInitialLoading(false)
    }
  }, [buildFilter, fetchCount, fetchItems])

  // Silently refresh items list (no loading flash, preserves expanded state)
  const refreshItemsList = useCallback(async () => {
    const filter = buildFilter()
    if (!filter) return
    try {
      await fetchItems(filter)
    } catch {
      setError('Failed to refresh data')
    }
  }, [buildFilter, fetchItems])

  // On filter change: reset page and fetch count + items
  useEffect(() => {
    const key = `${selectedRootId}|${issueType}|${fileType}|${status}|${pathSearch}`
    if (!isInitialLoad.current && key !== lastFilterKeyRef.current) {
      setCurrentPage(1)
    }
    isInitialLoad.current = false
    lastFilterKeyRef.current = key
    fetchFilterData()
  }, [fetchFilterData, selectedRootId, issueType, fileType, status, pathSearch])


  // Re-fetch on task completion
  useEffect(() => {
    if (lastTaskCompletedAt) fetchFilterData()
  }, [lastTaskCompletedAt, fetchFilterData])

  // Fetch versions for a specific item when expanding
  const fetchVersions = useCallback(async (itemId: number) => {
    const filter = buildFilter()
    if (!filter) return
    try {
      const result = await fetchIntegrityVersions(itemId, filter, VERSIONS_PER_EXPAND)
      setExpandedData((prev) => new Map(prev).set(itemId, result))
    } catch {
      setError('Failed to load versions')
    }
  }, [buildFilter])

  const toggleExpanded = (itemId: number) => {
    if (expandedData.has(itemId)) {
      setExpandedData((prev) => { const n = new Map(prev); n.delete(itemId); return n })
    } else {
      fetchVersions(itemId)
    }
  }

  // Refresh a single expanded item's versions after an action
  const refreshExpanded = useCallback(async (itemId: number) => {
    if (expandedData.has(itemId)) {
      await fetchVersions(itemId)
    }
  }, [expandedData, fetchVersions])

  // --- URL sync ---

  const syncUrl = useCallback((updates: Record<string, string>) => {
    setSearchParams((prev) => {
      const next = new URLSearchParams(prev)
      for (const [k, v] of Object.entries(updates)) {
        if (v && v !== 'all' && v !== 'unreviewed' && v !== '1') {
          next.set(k, v)
        } else {
          next.delete(k)
        }
      }
      return next
    }, { replace: true })
  }, [setSearchParams])

  // On page change only: fetch items (not count)
  const handlePageChange = useCallback((page: number) => {
    setCurrentPage(page)
    syncUrl({ page: String(page) })
  }, [syncUrl])

  const handleRootChange = useCallback((rootId: string) => {
    setSelectedRootId(rootId)
    setCurrentPage(1)
    syncUrl({ root_id: rootId, page: '1' })
  }, [syncUrl])

  const handleIssueTypeChange = useCallback((value: string) => {
    setIssueType(value)
    setCurrentPage(1)
    syncUrl({ issue_type: value, page: '1' })
  }, [syncUrl])

  const handleFileTypeChange = useCallback((value: string) => {
    setFileType(value)
    setCurrentPage(1)
    syncUrl({ file_type: value, page: '1' })
  }, [syncUrl])

  const handleStatusChange = useCallback((value: string) => {
    setStatus(value)
    setCurrentPage(1)
    syncUrl({ status: value, page: '1' })
  }, [syncUrl])

  const handlePathSearchChange = useCallback((value: string) => {
    setPathSearch(value)
    setCurrentPage(1)
    syncUrl({ q: value, page: '1' })
  }, [syncUrl])

  // --- Actions ---

  // Determine which review flags to set based on active issue_type filter
  const reviewFlags = useCallback((setTo: boolean): { set_val: boolean | null; set_hash: boolean | null } => {
    const it = issueType === 'all' ? 'all' : issueType
    return {
      set_val: it === 'all' || it === 'val' ? setTo : null,
      set_hash: it === 'all' || it === 'hash' ? setTo : null,
    }
  }, [issueType])

  const withPending = async (key: string, fn: () => Promise<void>) => {
    setPendingOps((s) => new Set(s).add(key))
    try {
      await fn()
    } catch {
      setError('Operation failed')
    } finally {
      setPendingOps((s) => { const n = new Set(s); n.delete(key); return n })
    }
  }

  // Review all versions of an item
  const handleReviewAll = async (item: IntegrityItemSummary) => {
    const flags = reviewFlags(true)
    await withPending(`review-all-${item.item_id}`, async () => {
      await setIntegrityReviewed(item.item_id, null, flags.set_val, flags.set_hash)
      await Promise.all([refreshItemsList(), refreshExpanded(item.item_id)])
    })
  }

  // Toggle hash review on a specific version
  const handleToggleHashReview = async (itemId: number, ver: IntegrityVersion) => {
    const setTo = ver.hash_reviewed_at === null
    await withPending(`${itemId}-${ver.item_version}-hash`, async () => {
      await setIntegrityReviewed(itemId, ver.item_version, null, setTo)
      await Promise.all([refreshItemsList(), refreshExpanded(itemId)])
    })
  }

  // Toggle val review on a specific version
  const handleToggleValReview = async (itemId: number, ver: IntegrityVersion) => {
    const setTo = ver.val_reviewed_at === null
    await withPending(`${itemId}-${ver.item_version}-val`, async () => {
      await setIntegrityReviewed(itemId, ver.item_version, setTo, null)
      await Promise.all([refreshItemsList(), refreshExpanded(itemId)])
    })
  }

  const handleToggleValidation = async (item: IntegrityItemSummary) => {
    await withPending(String(item.item_id), async () => {
      await setDoNotValidate(item.item_id, !item.do_not_validate)
      await refreshItemsList()
    })
  }

  const openDetail = (item: IntegrityItemSummary) => {
    setDetailItemId(item.item_id)
    setDetailItemPath(item.item_path)
    setDetailScanId(item.latest_scan_id)
    setDetailOpen(true)
  }

  // --- Render ---

  const totalPages = Math.ceil(total / ITEMS_PER_PAGE)
  const offset = (currentPage - 1) * ITEMS_PER_PAGE

  const actionBar = (
    <>
      <Select value={issueType} onValueChange={handleIssueTypeChange}>
        <SelectTrigger className="w-[180px]">
          <SelectValue placeholder="Issue type" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">All issue types</SelectItem>
          <SelectItem value="hash">Suspicious hashes</SelectItem>
          <SelectItem value="val">Validation errors</SelectItem>
        </SelectContent>
      </Select>

      <Select value={fileType} onValueChange={handleFileTypeChange}>
        <SelectTrigger className="w-[160px]">
          <SelectValue placeholder="File type" />
        </SelectTrigger>
        <SelectContent>
          {FILE_TYPE_OPTIONS.map((opt) => (
            <SelectItem key={opt.value} value={opt.value}>
              {opt.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>

      <Select value={status} onValueChange={handleStatusChange}>
        <SelectTrigger className="w-[160px]">
          <SelectValue placeholder="Status" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="unreviewed"><span className="inline-flex items-center gap-1.5"><CircleHelp className="h-3.5 w-3.5" />Not Reviewed</span></SelectItem>
          <SelectItem value="reviewed"><span className="inline-flex items-center gap-1.5"><CircleCheckBig className="h-3.5 w-3.5" />Reviewed</span></SelectItem>
          <SelectItem value="all">All</SelectItem>
        </SelectContent>
      </Select>

      <SearchFilter
        value={pathSearch}
        onChange={handlePathSearchChange}
        placeholder="Search path..."
      />
    </>
  )

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Integrity</h1>

      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={handleRootChange}
        actionBar={actionBar}
      >
        {error && (
          <p className="text-sm text-destructive">{error}</p>
        )}

        {!selectedRootId ? (
          <p className="text-sm text-muted-foreground">Select a root to view integrity issues.</p>
        ) : initialLoading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : items.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {status === 'unreviewed'
              ? 'All integrity issues have been reviewed.'
              : 'No integrity issues found.'}
          </p>
        ) : (
          <>
            <div className="border border-border rounded-lg overflow-hidden">
            <Table>
              <TableHeader className="bg-muted">
                <TableRow>
                  <TableHead className="w-[30px]" />
                  <TableHead className="w-[60px] uppercase text-xs tracking-wide">Validate</TableHead>
                  <TableHead className="uppercase text-xs tracking-wide">File</TableHead>
                  <TableHead className="w-[110px] uppercase text-xs tracking-wide">Hashes</TableHead>
                  <TableHead className="w-[110px] uppercase text-xs tracking-wide">Validation</TableHead>
                  <TableHead className="w-[100px]" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item) => {
                  const versionData = expandedData.get(item.item_id)
                  const isExpanded = versionData !== undefined
                  const validateInFlight = pendingOps.has(String(item.item_id))
                  const hasUnreviewed = item.hash_unreviewed + item.val_unreviewed > 0

                  return (
                    <Fragment key={item.item_id}>
                      {/* Summary row */}
                      <TableRow
                        className="cursor-pointer"
                        onClick={() => toggleExpanded(item.item_id)}
                      >
                        <TableCell className="px-2">
                          <ChevronDown className={cn(
                            "h-3.5 w-3.5 text-muted-foreground transition-transform",
                            !isExpanded && "-rotate-90"
                          )} />
                        </TableCell>
                        <TableCell className="px-2" onClick={(e) => e.stopPropagation()}>
                          <HoverCard openDelay={300}>
                            <HoverCardTrigger asChild>
                              <div className="flex items-center gap-1 cursor-default">
                                {item.do_not_validate
                                  ? <ShieldOff className="h-5 w-5 text-amber-500" />
                                  : <ShieldCheck className="h-5 w-5 text-muted-foreground" />
                                }
                                <Switch
                                  size="sm"
                                  checked={!item.do_not_validate}
                                  onCheckedChange={() => handleToggleValidation(item)}
                                  disabled={validateInFlight}
                                  className="data-[state=checked]:bg-muted-foreground"
                                  aria-label={item.do_not_validate ? 'Validation disabled' : 'Validation enabled'}
                                />
                              </div>
                            </HoverCardTrigger>
                            <HoverCardContent side="bottom" align="start" className="w-56 text-xs">
                              {item.do_not_validate
                                ? <p>Validation is <span className="font-semibold">disabled</span> for this file. Enable to include it in future validation scans.</p>
                                : <p>This file will be <span className="font-semibold">validated</span> during future scans. Disable to skip validation for this file.</p>
                              }
                            </HoverCardContent>
                          </HoverCard>
                        </TableCell>
                        <TableCell>
                          <div className="flex items-baseline gap-1 min-w-0">
                            <button
                              className="text-sm font-medium hover:underline hover:text-primary truncate text-left"
                              title={item.item_path}
                              onClick={(e) => { e.stopPropagation(); openDetail(item) }}
                            >
                              {item.item_name}
                            </button>
                            <span className="text-xs text-muted-foreground shrink-0">
                              in {parentFolder(item.item_path)}
                            </span>
                          </div>
                        </TableCell>
                        <TableCell className="text-xs">
                          <CountPair unreviewed={item.hash_unreviewed} reviewed={item.hash_reviewed} />
                        </TableCell>
                        <TableCell className="text-xs">
                          <CountPair unreviewed={item.val_unreviewed} reviewed={item.val_reviewed} />
                        </TableCell>
                        <TableCell className="px-2" onClick={(e) => e.stopPropagation()}>
                          <Button
                            variant="default"
                            size="sm"
                            className="h-7 text-xs gap-1"
                            onClick={() => handleReviewAll(item)}
                            disabled={!hasUnreviewed || pendingOps.has(`review-all-${item.item_id}`)}
                          >
                            <CircleCheckBig className="h-3.5 w-3.5" />
                            Review All
                          </Button>
                        </TableCell>
                      </TableRow>

                      {/* Expanded version rows */}
                      {isExpanded && (
                        <TableRow key={`${item.item_id}-detail`} className="hover:bg-transparent">
                          <TableCell colSpan={6} className="p-0 pl-10 pr-4 py-3">
                            <div className="border border-border rounded-lg overflow-hidden text-xs">
                              <Table className="table-fixed">
                                <TableHeader className="bg-muted">
                                  <TableRow>
                                    <TableHead className="w-[70px] text-xs py-1.5 h-auto">Version</TableHead>
                                    <TableHead className="w-[60px] text-xs py-1.5 h-auto">Scan</TableHead>
                                    <TableHead className="w-[90px] text-xs py-1.5 h-auto">When</TableHead>
                                    <TableHead className="w-[220px] text-xs py-1.5 h-auto">Hashes</TableHead>
                                    <TableHead className="text-xs py-1.5 h-auto">Validation</TableHead>
                                  </TableRow>
                                </TableHeader>
                                <TableBody>
                                  {versionData.versions.map((ver) => {
                                    const hashInFlight = pendingOps.has(`${item.item_id}-${ver.item_version}-hash`)
                                    const valInFlight = pendingOps.has(`${item.item_id}-${ver.item_version}-val`)
                                    const hasSuspicious = ver.hash_suspicious_count > 0
                                    const hasValError = ver.val_state === 2
                                    const hashReviewed = ver.hash_reviewed_at !== null
                                    const valReviewed = ver.val_reviewed_at !== null

                                    // Hash cell: suspicious > 0 → show count + toggle
                                    //            hash_version_count > 0 but no suspicious → "Baseline"
                                    //            hash_version_count === 0 → "Not hashed"
                                    let hashContent: React.ReactNode
                                    if (hasSuspicious) {
                                      hashContent = (
                                        <span className="inline-flex items-center gap-1.5">
                                          <HoverCard openDelay={300}>
                                            <HoverCardTrigger asChild>
                                              <span>
                                                <ReviewToggle
                                                  size="sm"
                                                  reviewed={hashReviewed}
                                                  onToggle={() => handleToggleHashReview(item.item_id, ver)}
                                                  disabled={hashInFlight}
                                                />
                                              </span>
                                            </HoverCardTrigger>
                                            <HoverCardContent side="bottom" className="w-56 text-xs">
                                              {hashReviewed
                                                ? <p>Mark this suspect hash as <span className="font-semibold">unreviewed</span></p>
                                                : <p>Mark this suspect hash as <span className="font-semibold">reviewed</span></p>
                                              }
                                            </HoverCardContent>
                                          </HoverCard>
                                          <span>{ver.hash_suspicious_count} suspicious</span>
                                        </span>
                                      )
                                    } else if (ver.hash_version_count > 0) {
                                      hashContent = <span className="text-muted-foreground">Baseline</span>
                                    } else {
                                      hashContent = <span className="text-muted-foreground">Not hashed</span>
                                    }

                                    // Val cell: val_state === 2 → show error + toggle
                                    //           val_state === 1 → "Valid"
                                    //           val_state === 0 → "Not validated"
                                    let valContent: React.ReactNode
                                    if (hasValError) {
                                      valContent = (
                                        <span className="inline-flex items-center gap-1.5 max-w-full">
                                          <HoverCard openDelay={300}>
                                            <HoverCardTrigger asChild>
                                              <span>
                                                <ReviewToggle
                                                  size="sm"
                                                  reviewed={valReviewed}
                                                  onToggle={() => handleToggleValReview(item.item_id, ver)}
                                                  disabled={valInFlight}
                                                />
                                              </span>
                                            </HoverCardTrigger>
                                            <HoverCardContent side="bottom" className="w-56 text-xs">
                                              {valReviewed
                                                ? <p>Mark this validation error as <span className="font-semibold">unreviewed</span></p>
                                                : <p>Mark this validation error as <span className="font-semibold">reviewed</span></p>
                                              }
                                            </HoverCardContent>
                                          </HoverCard>
                                          <span className="truncate" title={ver.val_error!}>{ver.val_error}</span>
                                        </span>
                                      )
                                    } else if (ver.val_state === 1) {
                                      valContent = <span className="text-muted-foreground">Valid</span>
                                    } else {
                                      valContent = <span className="text-muted-foreground">Not validated</span>
                                    }

                                    return (
                                      <TableRow key={ver.item_version}>
                                        <TableCell className="py-1.5 text-muted-foreground text-xs">v{ver.item_version}</TableCell>
                                        <TableCell className="py-1.5 text-muted-foreground text-xs">{ver.scan_id}</TableCell>
                                        <TableCell className="py-1.5 text-muted-foreground text-xs whitespace-nowrap">{formatTimeAgo(ver.scan_started_at)}</TableCell>
                                        <TableCell className="py-1.5 text-xs">{hashContent}</TableCell>
                                        <TableCell className="py-1.5 text-xs">{valContent}</TableCell>
                                      </TableRow>
                                    )
                                  })}
                                  {versionData.total > versionData.versions.length && (
                                    <TableRow>
                                      <TableCell colSpan={5} className="py-1.5 text-muted-foreground text-xs">
                                        Showing {versionData.versions.length} of {versionData.total} versions
                                      </TableCell>
                                    </TableRow>
                                  )}
                                </TableBody>
                              </Table>
                            </div>
                          </TableCell>
                        </TableRow>
                      )}
                    </Fragment>
                  )
                })}
              </TableBody>
            </Table>
            </div>

            <div className="flex items-center justify-between text-sm text-muted-foreground">
              <Button
                variant="outline"
                size="sm"
                disabled={currentPage <= 1}
                onClick={() => handlePageChange(currentPage - 1)}
              >
                ← Prev
              </Button>
              <span>
                Showing {offset + 1}–{Math.min(offset + items.length, total)} of {total}
              </span>
              <Button
                variant="outline"
                size="sm"
                disabled={currentPage >= totalPages}
                onClick={() => handlePageChange(currentPage + 1)}
              >
                Next →
              </Button>
            </div>
          </>
        )}
      </RootCard>

      {detailItemId !== null && (
        <ItemDetail
          mode="sheet"
          itemId={detailItemId}
          itemPath={detailItemPath}
          itemType="F"
          isTombstone={false}
          scanId={detailScanId}
          open={detailOpen}
          onOpenChange={setDetailOpen}
          onItemChanged={refreshItemsList}
        />
      )}
    </div>
  )
}
