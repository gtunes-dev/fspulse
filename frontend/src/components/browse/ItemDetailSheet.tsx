import { useState, useEffect } from 'react'
import { File, Folder, FileX, FolderX, Calendar, HardDrive, Hash, ShieldAlert, ShieldCheck, ShieldQuestion, ChevronDown } from 'lucide-react'
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
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from '@/components/ui/collapsible'
import { fetchQuery, countQuery, fetchItemFolderSize } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateFull } from '@/lib/dateUtils'

interface ItemDetailSheetProps {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
  rootId: number
  open: boolean
  onOpenChange: (open: boolean) => void
}

interface ItemDetails {
  item_id: number
  item_path: string
  item_type: string
  is_ts: boolean
  mod_date: number
  file_size: number | null
  file_hash: string | null
  val_status: string | null
  val_error: string | null
}

interface Change {
  change_id: number
  scan_id: number
  change_type: string
  is_undelete: boolean
  meta_change: boolean
  mod_date_old: number | null
  mod_date_new: number | null
  file_size_old: number | null
  file_size_new: number | null
  hash_change: boolean
  hash_old: string | null
  hash_new: string | null
  val_change: boolean
  val_old: string | null
  val_new: string | null
}

interface Alert {
  alert_id: number
  scan_id: number
  alert_type: string
  alert_status: string
  val_error: string | null
  created: number
}

const CHANGES_PER_PAGE = 20
const ALERTS_PER_PAGE = 20

// Column specifications - extracted to avoid duplication
const CHANGE_COLUMNS: ColumnSpec[] = [
  { name: 'change_id', visible: true, sort_direction: 'desc', position: 0 },
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 1 },
  { name: 'change_type', visible: true, sort_direction: 'none', position: 2 },
  { name: 'is_undelete', visible: true, sort_direction: 'none', position: 3 },
  { name: 'meta_change', visible: true, sort_direction: 'none', position: 4 },
  { name: 'mod_date_old', visible: true, sort_direction: 'none', position: 5 },
  { name: 'mod_date_new', visible: true, sort_direction: 'none', position: 6 },
  { name: 'file_size_old', visible: true, sort_direction: 'none', position: 7 },
  { name: 'file_size_new', visible: true, sort_direction: 'none', position: 8 },
  { name: 'hash_change', visible: true, sort_direction: 'none', position: 9 },
  { name: 'hash_old', visible: true, sort_direction: 'none', position: 10 },
  { name: 'hash_new', visible: true, sort_direction: 'none', position: 11 },
  { name: 'val_change', visible: true, sort_direction: 'none', position: 12 },
  { name: 'val_old', visible: true, sort_direction: 'none', position: 13 },
  { name: 'val_new', visible: true, sort_direction: 'none', position: 14 },
]

const ALERT_COLUMNS: ColumnSpec[] = [
  { name: 'alert_id', visible: true, sort_direction: 'desc', position: 0 },
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 1 },
  { name: 'alert_type', visible: true, sort_direction: 'none', position: 2 },
  { name: 'alert_status', visible: true, sort_direction: 'none', position: 3 },
  { name: 'val_error', visible: true, sort_direction: 'none', position: 4 },
  { name: 'created_at', visible: true, sort_direction: 'none', position: 5 },
]

// Row parsing helpers - extracted to avoid duplication
function parseChangeRow(row: string[]): Change {
  return {
    change_id: parseInt(row[0]),
    scan_id: parseInt(row[1]),
    change_type: row[2],
    is_undelete: row[3] === 'T',
    meta_change: row[4] === 'T',
    mod_date_old: row[5] && row[5] !== '-' ? parseInt(row[5]) : null,
    mod_date_new: row[6] && row[6] !== '-' ? parseInt(row[6]) : null,
    file_size_old: row[7] && row[7] !== '-' ? parseInt(row[7]) : null,
    file_size_new: row[8] && row[8] !== '-' ? parseInt(row[8]) : null,
    hash_change: row[9] === 'T',
    hash_old: row[10] && row[10] !== '-' ? row[10] : null,
    hash_new: row[11] && row[11] !== '-' ? row[11] : null,
    val_change: row[12] === 'T',
    val_old: row[13] && row[13] !== '-' ? row[13] : null,
    val_new: row[14] && row[14] !== '-' ? row[14] : null,
  }
}

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

export function ItemDetailSheet({
  itemId,
  itemPath,
  itemType,
  isTombstone,
  open,
  onOpenChange,
}: ItemDetailSheetProps) {
  const [loading, setLoading] = useState(false)
  const [details, setDetails] = useState<ItemDetails | null>(null)
  const [changes, setChanges] = useState<Change[]>([])
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [totalChanges, setTotalChanges] = useState(0)
  const [totalAlerts, setTotalAlerts] = useState(0)
  const [loadingMoreChanges, setLoadingMoreChanges] = useState(false)
  const [loadingMoreAlerts, setLoadingMoreAlerts] = useState(false)
  const [openChanges, setOpenChanges] = useState<Record<number, boolean>>({})
  const [folderSize, setFolderSize] = useState<number | null>(null)

  // Extract file/folder name from path
  const itemName = itemPath.split('/').filter(Boolean).pop() || itemPath

  // Reset collapsible state when switching items
  useEffect(() => {
    setOpenChanges({})
  }, [itemId])

  useEffect(() => {
    if (!open) return

    async function loadItemDetails() {
      setLoading(true)
      try {
        // Load item details
        const itemColumns: ColumnSpec[] = [
          { name: 'item_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'item_path', visible: true, sort_direction: 'none', position: 1 },
          { name: 'item_type', visible: true, sort_direction: 'none', position: 2 },
          { name: 'is_ts', visible: true, sort_direction: 'none', position: 3 },
          { name: 'mod_date', visible: true, sort_direction: 'none', position: 4 },
          { name: 'file_size', visible: true, sort_direction: 'none', position: 5 },
          { name: 'file_hash', visible: true, sort_direction: 'none', position: 6 },
          { name: 'val', visible: true, sort_direction: 'none', position: 7 },
          { name: 'val_error', visible: true, sort_direction: 'none', position: 8 },
        ]

        const itemResponse = await fetchQuery('items', {
          columns: itemColumns,
          filters: [{ column: 'item_id', value: itemId.toString() }],
          limit: 1,
          offset: 0,
        })

        if (itemResponse.rows.length > 0) {
          const row = itemResponse.rows[0]
          setDetails({
            item_id: parseInt(row[0]),
            item_path: row[1],
            item_type: row[2],
            is_ts: row[3] === 'T',
            mod_date: parseInt(row[4] || '0'),
            file_size: row[5] && row[5] !== '-' ? parseInt(row[5]) : null,
            file_hash: row[6] && row[6] !== '-' ? row[6] : null,
            val_status: row[7] && row[7] !== '-' ? row[7] : null,
            val_error: row[8] && row[8] !== '-' ? row[8] : null,
          })
        }

        // Count total changes
        const changeCountResponse = await countQuery('changes', {
          columns: [{ name: 'change_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [{ column: 'item_id', value: itemId.toString() }],
        })
        setTotalChanges(changeCountResponse.count)

        // Load initial changes (most recent first)
        const changeResponse = await fetchQuery('changes', {
          columns: CHANGE_COLUMNS,
          filters: [{ column: 'item_id', value: itemId.toString() }],
          limit: CHANGES_PER_PAGE,
          offset: 0,
        })

        setChanges(changeResponse.rows.map(parseChangeRow))

        // Count total alerts
        const alertCountResponse = await countQuery('alerts', {
          columns: [{ name: 'alert_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [{ column: 'item_id', value: itemId.toString() }],
        })
        setTotalAlerts(alertCountResponse.count)

        // Load initial alerts (most recent first)
        const alertResponse = await fetchQuery('alerts', {
          columns: ALERT_COLUMNS,
          filters: [{ column: 'item_id', value: itemId.toString() }],
          limit: ALERTS_PER_PAGE,
          offset: 0,
        })

        setAlerts(alertResponse.rows.map(parseAlertRow))

        // Load folder size if this is a directory
        if (itemType === 'D') {
          try {
            const sizeResponse = await fetchItemFolderSize(itemId)
            setFolderSize(sizeResponse.size)
          } catch (error) {
            console.error('Error loading folder size:', error)
            setFolderSize(null)
          }
        } else {
          setFolderSize(null)
        }
      } catch (error) {
        console.error('Error loading item details:', error)
      } finally {
        setLoading(false)
      }
    }

    loadItemDetails()
  }, [open, itemId, itemType])

  const loadMoreChanges = async () => {
    setLoadingMoreChanges(true)
    try {
      const changeResponse = await fetchQuery('changes', {
        columns: CHANGE_COLUMNS,
        filters: [{ column: 'item_id', value: itemId.toString() }],
        limit: CHANGES_PER_PAGE,
        offset: changes.length,
      })

      const newChanges = changeResponse.rows.map(parseChangeRow)
      setChanges([...changes, ...newChanges])
    } catch (error) {
      console.error('Error loading more changes:', error)
    } finally {
      setLoadingMoreChanges(false)
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
      setAlerts([...alerts, ...newAlerts])
    } catch (error) {
      console.error('Error loading more alerts:', error)
    } finally {
      setLoadingMoreAlerts(false)
    }
  }

  const formatFileSize = (bytes: number | null): string => {
    if (bytes === null) return 'N/A'
    if (bytes === 0) return '0 B'
    const units = ['B', 'KB', 'MB', 'GB', 'TB']
    let size = bytes
    let unitIndex = 0
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024
      unitIndex++
    }
    return `${size.toFixed(2)} ${units[unitIndex]}`
  }

  const getValidationBadge = (status: string | null) => {
    if (!status) return null
    switch (status) {
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

  const getChangeTypeBadge = (type: string) => {
    switch (type) {
      case 'A':
        return <Badge variant="success">Added</Badge>
      case 'M':
        return <Badge className="bg-amber-500 hover:bg-amber-600">Modified</Badge>
      case 'D':
        return <Badge variant="destructive">Deleted</Badge>
      default:
        return <Badge variant="secondary">No Change</Badge>
    }
  }

  const getAlertTypeBadge = (type: string) => {
    switch (type) {
      case 'H':
        return <Badge variant="destructive">Suspicious Hash</Badge>
      case 'I':
        return <Badge variant="destructive">Invalid Item</Badge>
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
              {isTombstone && (
                <div className="mt-2 flex items-center gap-2">
                  <Badge variant="destructive" className="text-base px-3 py-1">Deleted Item</Badge>
                  <span className="text-sm text-muted-foreground">This item no longer exists</span>
                </div>
              )}
            </div>
          </div>
        </SheetHeader>

        {loading ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading details...</p>
          </div>
        ) : details ? (
          <div className="space-y-6 mt-6">
            {/* Beautiful Summary Section */}
            <Card className="border-2">
              <CardHeader>
                <CardTitle className="text-lg">Current State</CardTitle>
              </CardHeader>
              <CardContent>
                <div className="grid grid-cols-2 gap-4">
                  <div>
                    <p className="text-sm font-medium text-muted-foreground">Item ID</p>
                    <p className="text-base font-semibold mt-1 font-mono">{details.item_id}</p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                      <HardDrive className="h-4 w-4" />
                      Type
                    </p>
                    <p className="text-base font-semibold mt-1">
                      {details.item_type === 'F' ? 'File' : details.item_type === 'D' ? 'Directory' : details.item_type === 'S' ? 'Symlink' : 'Other'}
                    </p>
                  </div>
                  <div>
                    <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                      <Calendar className="h-4 w-4" />
                      Modified
                    </p>
                    <p className="text-base font-semibold mt-1">
                      {details.mod_date ? formatDateFull(details.mod_date) : 'N/A'}
                    </p>
                  </div>
                  {details.item_type === 'F' && details.file_size !== null && (
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Size</p>
                      <p className="text-base font-semibold mt-1">{formatFileSize(details.file_size)}</p>
                    </div>
                  )}
                  {details.item_type === 'D' && folderSize !== null && (
                    <div>
                      <p className="text-sm font-medium text-muted-foreground">Total Size</p>
                      <p className="text-base font-semibold mt-1">{formatFileSize(folderSize)}</p>
                    </div>
                  )}
                  {details.item_type === 'F' && (
                    <div>
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <ShieldCheck className="h-4 w-4" />
                        Validation
                      </p>
                      <div className="mt-1">{getValidationBadge(details.val_status)}</div>
                    </div>
                  )}
                  {details.item_type === 'F' && details.file_hash && details.file_hash !== '-' && (
                    <div className="col-span-2">
                      <p className="text-sm font-medium text-muted-foreground flex items-center gap-2">
                        <Hash className="h-4 w-4" />
                        Hash
                      </p>
                      <p className="text-xs font-mono mt-1 break-all">{details.file_hash}</p>
                    </div>
                  )}
                  {details.item_type === 'F' && details.val_error && details.val_error.trim() !== '' && details.val_error !== '-' && (
                    <div className="col-span-2">
                      <p className="text-sm font-medium text-destructive">Validation Error</p>
                      <p className="text-xs font-mono mt-1 bg-destructive/10 p-2 rounded">{details.val_error}</p>
                    </div>
                  )}
                </div>
              </CardContent>
            </Card>

            <Separator />

            {/* Changes Section - Single Card Container */}
            <div>
              <div className="flex items-center justify-between mb-3">
                <h3 className="text-lg font-semibold">History</h3>
                {totalChanges > CHANGES_PER_PAGE && (
                  <p className="text-sm text-muted-foreground">
                    Showing {changes.length} of {totalChanges} change{totalChanges !== 1 ? 's' : ''}
                  </p>
                )}
              </div>
              {totalChanges === 0 ? (
                <p className="text-sm text-muted-foreground">No changes recorded</p>
              ) : (
                <>
                  <Card className="shadow-md">
                    <CardContent className="p-0">
                      {changes.map((change, idx) => {
                        const isOpen = openChanges[change.change_id] || false
                        const setIsOpen = (open: boolean) => {
                          setOpenChanges(prev => ({ ...prev, [change.change_id]: open }))
                        }

                        return (
                          <div key={change.change_id}>
                            <div className="p-4">
                              {change.change_type === 'M' && (() => {
                                const hasMetaChanges = change.meta_change && (change.mod_date_old !== change.mod_date_new || change.file_size_old !== change.file_size_new)
                                const hasHashChanges = change.hash_change && change.hash_old !== change.hash_new
                                const hasValChanges = change.val_change && change.val_old !== change.val_new
                                const hasAnyChanges = hasMetaChanges || hasHashChanges || hasValChanges

                                if (!hasAnyChanges) {
                                  return (
                                    <div className="flex items-center gap-2">
                                      {/* Invisible spacer to align badges with expandable rows */}
                                      <div className="h-5 w-5 flex-shrink-0" />
                                      {getChangeTypeBadge(change.change_type)}
                                      <p className="text-xs text-muted-foreground">
                                        Scan <span className="font-mono font-semibold">#{change.scan_id}</span>
                                        <span className="mx-2">•</span>
                                        Change <span className="font-mono font-semibold">#{change.change_id}</span>
                                      </p>
                                    </div>
                                  )
                                }

                                return (
                                  <Collapsible open={isOpen} onOpenChange={setIsOpen}>
                                    <div className="flex items-center gap-2">
                                      <CollapsibleTrigger asChild>
                                        <Button variant="ghost" size="icon" className="h-5 w-5 p-0 flex-shrink-0">
                                          <ChevronDown
                                            className={`h-3 w-3 transition-transform duration-200 ${isOpen ? '' : '-rotate-90'}`}
                                          />
                                        </Button>
                                      </CollapsibleTrigger>
                                      {getChangeTypeBadge(change.change_type)}
                                      <p className="text-xs text-muted-foreground">
                                        Scan <span className="font-mono font-semibold">#{change.scan_id}</span>
                                        <span className="mx-2">•</span>
                                        Change <span className="font-mono font-semibold">#{change.change_id}</span>
                                      </p>
                                    </div>
                                    <CollapsibleContent className="mt-2 ml-7">
                                      <div className="space-y-2 text-xs">
                                        {change.meta_change && change.mod_date_old !== change.mod_date_new && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <Calendar className="h-3 w-3" />
                                              Modification Date
                                            </p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">
                                                {change.mod_date_old && change.mod_date_old !== 0 ? formatDateFull(change.mod_date_old) : 'N/A'}
                                              </span>
                                              <span className="text-muted-foreground">→</span>
                                              <span className="font-medium">
                                                {change.mod_date_new && change.mod_date_new !== 0 ? formatDateFull(change.mod_date_new) : 'N/A'}
                                              </span>
                                            </div>
                                          </div>
                                        )}

                                        {change.meta_change && change.file_size_old !== change.file_size_new && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <HardDrive className="h-3 w-3" />
                                              File Size
                                            </p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">
                                                {formatFileSize(change.file_size_old)}
                                              </span>
                                              <span className="text-muted-foreground">→</span>
                                              <span className="font-medium">
                                                {formatFileSize(change.file_size_new)}
                                              </span>
                                            </div>
                                          </div>
                                        )}

                                        {change.hash_change && change.hash_old !== change.hash_new && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <Hash className="h-3 w-3" />
                                              File Hash
                                            </p>
                                            <div className="space-y-1">
                                              <div className="flex items-start gap-2">
                                                <span className="text-muted-foreground flex-shrink-0">Old:</span>
                                                <span className="font-mono break-all text-muted-foreground">
                                                  {change.hash_old && change.hash_old !== '-' ? change.hash_old : 'N/A'}
                                                </span>
                                              </div>
                                              <div className="flex items-start gap-2">
                                                <span className="text-muted-foreground flex-shrink-0">New:</span>
                                                <span className="font-mono break-all font-medium">
                                                  {change.hash_new && change.hash_new !== '-' ? change.hash_new : 'N/A'}
                                                </span>
                                              </div>
                                            </div>
                                          </div>
                                        )}

                                        {change.val_change && change.val_old !== change.val_new && (
                                          <div className="bg-muted/50 p-2 rounded">
                                            <p className="font-medium mb-1 flex items-center gap-1">
                                              <ShieldCheck className="h-3 w-3" />
                                              Validation Status
                                            </p>
                                            <div className="flex items-center gap-2">
                                              <span className="text-muted-foreground">
                                                {change.val_old && change.val_old !== '-' ? getValidationBadge(change.val_old) : 'N/A'}
                                              </span>
                                              <span className="text-muted-foreground">→</span>
                                              <span className="font-medium">
                                                {change.val_new && change.val_new !== '-' ? getValidationBadge(change.val_new) : 'N/A'}
                                              </span>
                                            </div>
                                          </div>
                                        )}
                                      </div>
                                    </CollapsibleContent>
                                  </Collapsible>
                                )
                              })()}

                              {/* Non-modified changes (Add/Delete) - no collapsible */}
                              {change.change_type !== 'M' && (
                                <div className="flex items-center gap-2">
                                  {/* Invisible spacer to align badges with expandable rows */}
                                  <div className="h-5 w-5 flex-shrink-0" />
                                  {getChangeTypeBadge(change.change_type)}
                                  <p className="text-xs text-muted-foreground">
                                    Scan <span className="font-mono font-semibold">#{change.scan_id}</span>
                                    <span className="mx-2">•</span>
                                    Change <span className="font-mono font-semibold">#{change.change_id}</span>
                                  </p>
                                </div>
                              )}
                            </div>
                            {idx < changes.length - 1 && <Separator />}
                          </div>
                        )
                      })}
                    </CardContent>
                  </Card>
                  {totalChanges > changes.length && changes.length >= CHANGES_PER_PAGE && (
                    <div className="mt-4 flex justify-center">
                      <Button
                        variant="outline"
                        onClick={loadMoreChanges}
                        disabled={loadingMoreChanges}
                      >
                        {loadingMoreChanges ? 'Loading...' : `Load ${Math.min(CHANGES_PER_PAGE, totalChanges - changes.length)} more`}
                      </Button>
                    </div>
                  )}
                </>
              )}
            </div>

            <Separator />

            {/* Alerts Section - Single Card Container */}
            <div>
              <div className="flex items-center justify-between mb-3">
                <h3 className="text-lg font-semibold">Alerts</h3>
                {totalAlerts > ALERTS_PER_PAGE && (
                  <p className="text-sm text-muted-foreground">
                    Showing {alerts.length} of {totalAlerts} alert{totalAlerts !== 1 ? 's' : ''}
                  </p>
                )}
              </div>
              {totalAlerts === 0 ? (
                <p className="text-sm text-muted-foreground">No alerts for this item</p>
              ) : (
                <>
                  <Card className="shadow-md">
                    <CardContent className="p-0">
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
                    </CardContent>
                  </Card>
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
            </div>
          </div>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
