import { useState, useEffect, useCallback, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Check } from 'lucide-react'
import { Button } from '@/components/ui/button'
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
import { fetchIntegrity, acknowledgeIntegrity, setDoNotValidate, fetchQuery } from '@/lib/api'
import type { IntegrityItem } from '@/lib/api'
import { formatTimeAgo } from '@/lib/dateUtils'
import { useTaskContext } from '@/contexts/TaskContext'

interface Root {
  root_id: number
  root_path: string
}

const ITEMS_PER_PAGE = 50

// Extension groups for the file type filter.
// Each entry maps a display label to a comma-separated extension list for the API.
const FILE_TYPE_OPTIONS: { label: string; value: string }[] = [
  { label: 'All file types', value: 'all' },
  { label: 'Image files', value: 'jpg,jpeg,png,gif,bmp,tiff' },
  { label: 'PDF files', value: 'pdf' },
  { label: 'Audio files', value: 'flac' },
]

function issueLabel(item: IntegrityItem): { hash: string; val: string } {
  const hashAck = item.hash_acknowledged_at !== null
  const valAck = item.val_acknowledged_at !== null

  return {
    hash: item.hash_state === 2 ? (hashAck ? 'Suspect ✓' : 'Suspect') : '—',
    val: item.val_state === 2 ? (valAck ? 'Invalid ✓' : 'Invalid') : '—',
  }
}

function rowNeedsAction(item: IntegrityItem): boolean {
  return (
    (item.hash_state === 2 && item.hash_acknowledged_at === null) ||
    (item.val_state === 2 && item.val_acknowledged_at === null)
  )
}

export function IntegrityPage() {
  const { lastTaskCompletedAt } = useTaskContext()
  const [searchParams, setSearchParams] = useSearchParams()

  const initialRootId = searchParams.get('root_id') || ''
  const [selectedRootId, setSelectedRootId] = useState<string>(initialRootId)
  const [issueType, setIssueType] = useState<string>('all')
  const [fileType, setFileType] = useState<string>('all')
  const [status, setStatus] = useState<string>('unacknowledged')
  const [pathSearch, setPathSearch] = useState<string>('')
  const [currentPage, setCurrentPage] = useState(1)

  const [roots, setRoots] = useState<Root[]>([])
  const [items, setItems] = useState<IntegrityItem[]>([])
  const [total, setTotal] = useState(0)
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Item detail sheet
  const [detailItem, setDetailItem] = useState<IntegrityItem | null>(null)
  const [detailOpen, setDetailOpen] = useState(false)

  // Pending acknowledge/dnv actions (item_id → true while in flight)
  const [pendingAck, setPendingAck] = useState<Set<number>>(new Set())

  const isInitialLoad = useRef(true)
  const lastFilterKeyRef = useRef<string>('')

  // Load roots once — same pattern as HistoryPage / AlertsPage
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
        if (!selectedRootId && loaded.length > 0) {
          setSelectedRootId(String(loaded[0].root_id))
        }
      })
      .catch(() => setError('Failed to load roots'))
  }, [])

  // Fetch integrity items whenever filters or page change
  const fetchItems = useCallback(async () => {
    if (!selectedRootId) return

    const rootId = parseInt(selectedRootId)
    if (isNaN(rootId)) return

    setLoading(true)
    setError(null)

    try {
      const result = await fetchIntegrity({
        root_id: rootId,
        issue_type: issueType === 'all' ? undefined : issueType,
        extensions: fileType === 'all' ? undefined : fileType,
        status,
        path_search: pathSearch || undefined,
        offset: (currentPage - 1) * ITEMS_PER_PAGE,
        limit: ITEMS_PER_PAGE,
      })
      setItems(result.items)
      setTotal(result.total)
    } catch {
      setError('Failed to load integrity data')
    } finally {
      setLoading(false)
    }
  }, [selectedRootId, issueType, fileType, status, pathSearch, currentPage])

  // Reset to page 1 when filters change, then fetch
  useEffect(() => {
    const key = `${selectedRootId}|${issueType}|${fileType}|${status}|${pathSearch}`
    if (!isInitialLoad.current && key !== lastFilterKeyRef.current) {
      setCurrentPage(1)
    }
    isInitialLoad.current = false
    lastFilterKeyRef.current = key
    fetchItems()
  }, [fetchItems, selectedRootId, issueType, fileType, status, pathSearch])

  // Re-fetch when a task completes (a new scan may have found new issues)
  useEffect(() => {
    if (lastTaskCompletedAt) fetchItems()
  }, [lastTaskCompletedAt])

  const handleRootChange = (rootId: string) => {
    setSelectedRootId(rootId)
    setCurrentPage(1)
    if (rootId) {
      setSearchParams((prev) => { prev.set('root_id', rootId); return prev })
    }
  }

  const handleAcknowledge = async (item: IntegrityItem) => {
    const ackVal = item.val_state === 2 && item.val_acknowledged_at === null
    const ackHash = item.hash_state === 2 && item.hash_acknowledged_at === null

    setPendingAck((s) => new Set(s).add(item.item_id))
    try {
      await acknowledgeIntegrity(item.item_id, item.item_version, ackVal, ackHash)
      await fetchItems()
    } catch {
      setError('Failed to acknowledge')
    } finally {
      setPendingAck((s) => { const n = new Set(s); n.delete(item.item_id); return n })
    }
  }

  const handleDoNotValidate = async (item: IntegrityItem) => {
    setPendingAck((s) => new Set(s).add(item.item_id))
    try {
      await setDoNotValidate(item.item_id, !item.do_not_validate)
      await fetchItems()
    } catch {
      setError('Failed to update setting')
    } finally {
      setPendingAck((s) => { const n = new Set(s); n.delete(item.item_id); return n })
    }
  }

  const totalPages = Math.ceil(total / ITEMS_PER_PAGE)
  const offset = (currentPage - 1) * ITEMS_PER_PAGE

  const actionBar = (
    <>
      <Select value={issueType} onValueChange={setIssueType}>
        <SelectTrigger className="w-[180px]">
          <SelectValue placeholder="Issue type" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">All issue types</SelectItem>
          <SelectItem value="hash">Suspect hash</SelectItem>
          <SelectItem value="val">Invalid validation</SelectItem>
        </SelectContent>
      </Select>

      <Select value={fileType} onValueChange={setFileType}>
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

      <Select value={status} onValueChange={setStatus}>
        <SelectTrigger className="w-[180px]">
          <SelectValue placeholder="Status" />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="unacknowledged">Unacknowledged</SelectItem>
          <SelectItem value="acknowledged">Acknowledged</SelectItem>
          <SelectItem value="all">All</SelectItem>
        </SelectContent>
      </Select>

      <SearchFilter
        value={pathSearch}
        onChange={setPathSearch}
        placeholder="Search path..."
      />
    </>
  )

  return (
    <>
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
        ) : loading ? (
          <p className="text-sm text-muted-foreground">Loading…</p>
        ) : items.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            {status === 'unacknowledged'
              ? 'No unacknowledged integrity issues.'
              : 'No integrity issues found.'}
          </p>
        ) : (
          <>
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead>File</TableHead>
                  <TableHead className="w-[120px]">Hash</TableHead>
                  <TableHead className="w-[140px]">Validation</TableHead>
                  <TableHead className="w-[100px]">Found</TableHead>
                  <TableHead className="w-[200px]" />
                </TableRow>
              </TableHeader>
              <TableBody>
                {items.map((item) => {
                  const labels = issueLabel(item)
                  const needsAction = rowNeedsAction(item)
                  const inFlight = pendingAck.has(item.item_id)
                  return (
                    <TableRow key={item.item_id}>
                      <TableCell>
                        <button
                          className="text-left text-sm font-medium hover:underline truncate max-w-[320px] block"
                          title={item.item_path}
                          onClick={() => { setDetailItem(item); setDetailOpen(true) }}
                        >
                          {item.item_name}
                        </button>
                        <div className="text-xs text-muted-foreground truncate max-w-[320px]">
                          {item.item_path}
                        </div>
                      </TableCell>
                      <TableCell>
                        <span className={labels.hash === '—' ? 'text-muted-foreground' : item.hash_acknowledged_at ? 'text-muted-foreground' : 'text-amber-600 dark:text-amber-400 font-medium'}>
                          {labels.hash}
                        </span>
                      </TableCell>
                      <TableCell>
                        <span className={labels.val === '—' ? 'text-muted-foreground' : item.val_acknowledged_at ? 'text-muted-foreground' : 'text-rose-600 dark:text-rose-400 font-medium'}>
                          {labels.val}
                        </span>
                      </TableCell>
                      <TableCell className="text-sm text-muted-foreground">
                        {formatTimeAgo(item.first_detected_at)}
                      </TableCell>
                      <TableCell>
                        <div className="flex items-center gap-2 justify-end">
                          {needsAction && (
                            <Button
                              size="sm"
                              variant="outline"
                              disabled={inFlight}
                              onClick={() => handleAcknowledge(item)}
                            >
                              <Check className="h-3.5 w-3.5 mr-1" />
                              Acknowledge
                            </Button>
                          )}
                          {item.val_state === 2 && (
                            <Button
                              size="sm"
                              variant="ghost"
                              disabled={inFlight}
                              onClick={() => handleDoNotValidate(item)}
                              title={item.do_not_validate ? 'Re-enable validation for this file' : 'Stop validating this file'}
                            >
                              {item.do_not_validate ? 'Re-enable' : 'Do Not Validate'}
                            </Button>
                          )}
                        </div>
                      </TableCell>
                    </TableRow>
                  )
                })}
              </TableBody>
            </Table>

            {/* Pagination */}
            <div className="flex items-center justify-between text-sm text-muted-foreground">
              <Button
                variant="outline"
                size="sm"
                disabled={currentPage <= 1}
                onClick={() => setCurrentPage((p) => p - 1)}
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
                onClick={() => setCurrentPage((p) => p + 1)}
              >
                Next →
              </Button>
            </div>
          </>
        )}
      </RootCard>

      {detailItem && (
        <ItemDetail
          mode="sheet"
          itemId={detailItem.item_id}
          itemPath={detailItem.item_path}
          itemType="F"
          isTombstone={false}
          scanId={0}
          open={detailOpen}
          onOpenChange={setDetailOpen}
        />
      )}
    </>
  )
}
