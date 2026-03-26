import { useState, useEffect } from 'react'
import { Link } from 'react-router-dom'
import { Activity, Clock, Timer, CheckCircle, XCircle, AlertTriangle, Files, Folder, HardDrive, Hash, CircleX, FolderTree } from 'lucide-react'
import {
  Sheet,
  SheetContent,
  SheetHeader,
  SheetTitle,
} from '@/components/ui/sheet'
import { Badge } from '@/components/ui/badge'
import { Card, CardContent } from '@/components/ui/card'
import { Separator } from '@/components/ui/separator'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatDateTimeShort } from '@/lib/dateUtils'
import { formatFileSize } from '@/lib/formatUtils'
import { ChangeIcons } from '@/components/shared/ChangeIcons'

interface ScanDetailSheetProps {
  scanId: number
  open: boolean
  onOpenChange: (open: boolean) => void
  showBrowseLink?: boolean
}

interface ScanDetails {
  scan_id: number
  root_id: number
  root_path: string
  started_at: number
  ended_at: number | null
  was_restarted: boolean
  scan_state: string
  is_hash: boolean
  hash_all: boolean
  is_val: boolean
  add_count: number
  modify_count: number
  delete_count: number
  file_count: number
  folder_count: number
  total_size: number | null
  new_hash_suspect_count: number
  new_val_invalid_count: number
  val_unknown_count: number
  val_valid_count: number
  val_invalid_count: number
  val_no_validator_count: number
  hash_unknown_count: number
  hash_baseline_count: number
  hash_suspect_count: number
  error: string | null
}

const SCAN_COLUMNS: ColumnSpec[] = [
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
  { name: 'root_id', visible: true, sort_direction: 'none', position: 1 },
  { name: 'started_at', visible: true, sort_direction: 'none', position: 2 },
  { name: 'ended_at', visible: true, sort_direction: 'none', position: 3 },
  { name: 'was_restarted', visible: true, sort_direction: 'none', position: 4 },
  { name: 'scan_state', visible: true, sort_direction: 'none', position: 5 },
  { name: 'is_hash', visible: true, sort_direction: 'none', position: 6 },
  { name: 'hash_all', visible: true, sort_direction: 'none', position: 7 },
  { name: 'is_val', visible: true, sort_direction: 'none', position: 8 },
  { name: 'add_count', visible: true, sort_direction: 'none', position: 9 },
  { name: 'modify_count', visible: true, sort_direction: 'none', position: 10 },
  { name: 'delete_count', visible: true, sort_direction: 'none', position: 11 },
  { name: 'file_count', visible: true, sort_direction: 'none', position: 12 },
  { name: 'folder_count', visible: true, sort_direction: 'none', position: 13 },
  { name: 'total_size', visible: true, sort_direction: 'none', position: 14 },
  { name: 'new_hash_suspect_count', visible: true, sort_direction: 'none', position: 15 },
  { name: 'new_val_invalid_count', visible: true, sort_direction: 'none', position: 16 },
  { name: 'val_unknown_count', visible: true, sort_direction: 'none', position: 17 },
  { name: 'val_valid_count', visible: true, sort_direction: 'none', position: 18 },
  { name: 'val_invalid_count', visible: true, sort_direction: 'none', position: 19 },
  { name: 'val_no_validator_count', visible: true, sort_direction: 'none', position: 20 },
  { name: 'hash_unknown_count', visible: true, sort_direction: 'none', position: 21 },
  { name: 'hash_baseline_count', visible: true, sort_direction: 'none', position: 22 },
  { name: 'hash_suspect_count', visible: true, sort_direction: 'none', position: 23 },
  { name: 'error', visible: true, sort_direction: 'none', position: 24 },
]

const ROOT_COLUMNS: ColumnSpec[] = [
  { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
  { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
]

function parseBool(val: string): boolean {
  return val === 'T' || val === 'True' || val === '1' || val === 'true'
}

function parseIntOrNull(val: string): number | null {
  if (!val || val === '-') return null
  const n = parseInt(val)
  return isNaN(n) ? null : n
}

function formatDuration(startSeconds: number, endSeconds: number): string {
  const diff = endSeconds - startSeconds
  if (diff < 1) return '< 1 second'
  if (diff < 60) return `${diff} second${diff === 1 ? '' : 's'}`
  const mins = Math.floor(diff / 60)
  const secs = diff % 60
  if (mins < 60) return secs > 0 ? `${mins}m ${secs}s` : `${mins} minute${mins === 1 ? '' : 's'}`
  const hours = Math.floor(mins / 60)
  const remMins = mins % 60
  return remMins > 0 ? `${hours}h ${remMins}m` : `${hours} hour${hours === 1 ? '' : 's'}`
}

function getStatusBadge(state: string, wasRestarted: boolean) {
  const restartSuffix = wasRestarted ? ' (restarted)' : ''
  switch (state) {
    case 'C':
      return (
        <Badge variant="success" className="gap-1">
          <CheckCircle className="h-3 w-3" />
          Completed{restartSuffix}
        </Badge>
      )
    case 'E':
      return (
        <Badge variant="destructive" className="gap-1">
          <XCircle className="h-3 w-3" />
          Error{restartSuffix}
        </Badge>
      )
    case 'P':
      return (
        <Badge className="bg-amber-500 hover:bg-amber-600 gap-1">
          <AlertTriangle className="h-3 w-3" />
          Stopped{restartSuffix}
        </Badge>
      )
    default:
      return <Badge variant="secondary">{state}{restartSuffix}</Badge>
  }
}

export function ScanDetailSheet({
  scanId,
  open,
  onOpenChange,
  showBrowseLink = true,
}: ScanDetailSheetProps) {
  const [loading, setLoading] = useState(false)
  const [details, setDetails] = useState<ScanDetails | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return

    async function loadScanDetails() {
      setLoading(true)
      setError(null)
      try {
        const scanResponse = await fetchQuery('scans', {
          columns: SCAN_COLUMNS,
          filters: [{ column: 'scan_id', value: scanId.toString() }],
          limit: 1,
        })

        if (scanResponse.rows.length === 0) {
          setError(`Scan #${scanId} not found`)
          return
        }

        const row = scanResponse.rows[0]
        const rootId = parseInt(row[1])

        const rootResponse = await fetchQuery('roots', {
          columns: ROOT_COLUMNS,
          filters: [{ column: 'root_id', value: rootId.toString() }],
          limit: 1,
        })

        const rootPath = rootResponse.rows.length > 0 ? rootResponse.rows[0][1] : `Root ${rootId}`

        setDetails({
          scan_id: parseInt(row[0]),
          root_id: rootId,
          root_path: rootPath,
          started_at: parseInt(row[2]),
          ended_at: parseIntOrNull(row[3]),
          was_restarted: parseBool(row[4]),
          scan_state: row[5],
          is_hash: parseBool(row[6]),
          hash_all: parseBool(row[7]),
          is_val: parseBool(row[8]),
          add_count: parseInt(row[9]) || 0,
          modify_count: parseInt(row[10]) || 0,
          delete_count: parseInt(row[11]) || 0,
          file_count: parseInt(row[12]) || 0,
          folder_count: parseInt(row[13]) || 0,
          total_size: parseIntOrNull(row[14]),
          new_hash_suspect_count: parseInt(row[15]) || 0,
          new_val_invalid_count: parseInt(row[16]) || 0,
          val_unknown_count: parseInt(row[17]) || 0,
          val_valid_count: parseInt(row[18]) || 0,
          val_invalid_count: parseInt(row[19]) || 0,
          val_no_validator_count: parseInt(row[20]) || 0,
          hash_unknown_count: parseInt(row[21]) || 0,
          hash_baseline_count: parseInt(row[22]) || 0,
          hash_suspect_count: parseInt(row[23]) || 0,
          error: row[24] && row[24] !== '-' ? row[24] : null,
        })
      } catch (err) {
        setError(err instanceof Error ? err.message : 'Failed to load scan details')
      } finally {
        setLoading(false)
      }
    }

    loadScanDetails()
  }, [open, scanId])

  const hasChanges = details && (details.add_count > 0 || details.modify_count > 0 || details.delete_count > 0)

  return (
    <Sheet open={open} onOpenChange={onOpenChange}>
      <SheetContent side="right" className="!w-[500px] sm:!w-[540px] !max-w-[540px] overflow-y-auto">
        {/* Header */}
        <SheetHeader>
          <div className="flex items-start gap-3">
            <div className="flex-shrink-0 mt-0.5">
              <Activity className="h-8 w-8 text-muted-foreground" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="flex items-center gap-2">
                <SheetTitle className="text-lg font-bold break-words">Scan #{scanId}</SheetTitle>
                {showBrowseLink && details && (
                  <Link
                    to={`/browse?root_id=${details.root_id}&scan_id=${details.scan_id}`}
                    onClick={() => onOpenChange(false)}
                    className="text-muted-foreground hover:text-primary p-1 rounded hover:bg-accent transition-colors"
                    title="Browse this scan"
                  >
                    <FolderTree className="h-4 w-4" />
                  </Link>
                )}
              </div>
              {details && (
                <p className="text-sm text-muted-foreground break-all">in {details.root_path}</p>
              )}
            </div>
          </div>
        </SheetHeader>

        {loading ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-muted-foreground">Loading details...</p>
          </div>
        ) : error ? (
          <div className="flex items-center justify-center h-64">
            <p className="text-red-500">{error}</p>
          </div>
        ) : details ? (
          <Card className="mt-4">
            {/* Status + Time */}
            <CardContent className="py-3 space-y-2">
              <div className="flex items-center gap-3 text-sm">
                <span className="text-muted-foreground">Status:</span>
                {getStatusBadge(details.scan_state, details.was_restarted)}
              </div>

              <div className="flex items-center gap-1.5 text-sm">
                <Clock className="h-4 w-4 text-muted-foreground" />
                <span>{formatDateTimeShort(details.started_at)}</span>
              </div>

              {details.ended_at && (
                <div className="flex items-center gap-1.5 text-sm">
                  <Timer className="h-4 w-4 text-muted-foreground" />
                  <span>Duration: {formatDuration(details.started_at, details.ended_at)}</span>
                </div>
              )}

              {details.error && (
                <div className="text-sm text-destructive mt-1 p-2 rounded bg-destructive/10">
                  {details.error}
                </div>
              )}
            </CardContent>

            <Separator />

            {/* Items */}
            <CardContent className="py-3">
              <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">Contents</p>
              <div className="flex items-center gap-5 text-sm">
                <span className="inline-flex items-center gap-1.5">
                  <Files className="h-4 w-4 text-muted-foreground" />
                  <span className="font-medium">{details.file_count.toLocaleString()}</span>
                  <span className="text-muted-foreground">files</span>
                </span>
                <span className="inline-flex items-center gap-1.5">
                  <Folder className="h-4 w-4 text-muted-foreground" />
                  <span className="font-medium">{details.folder_count.toLocaleString()}</span>
                  <span className="text-muted-foreground">folders</span>
                </span>
                {details.total_size !== null && (
                  <span className="inline-flex items-center gap-1.5">
                    <HardDrive className="h-4 w-4 text-muted-foreground" />
                    <span className="font-medium">{formatFileSize(details.total_size)}</span>
                  </span>
                )}
              </div>
            </CardContent>

            <Separator />

            {/* Changes */}
            <CardContent className="py-3">
              <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">Changes</p>
              <div className="text-sm">
                {hasChanges ? (
                  <ChangeIcons add={details.add_count} modify={details.modify_count} del={details.delete_count} />
                ) : (
                  <span className="text-muted-foreground">No changes</span>
                )}
              </div>
            </CardContent>

            <Separator />

            {/* Integrity */}
            <CardContent className="py-3">
              <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-2">Integrity</p>

              {/* Config line */}
              <div className="flex items-center gap-4 text-sm mb-3">
                <span>
                  <span className="text-muted-foreground">Hashing:</span>{' '}
                  <span className="font-medium">{details.is_hash ? (details.hash_all ? 'All' : 'New / Changed') : 'None'}</span>
                </span>
                <span>
                  <span className="text-muted-foreground">Validation:</span>{' '}
                  <span className="font-medium">{details.is_val ? 'Enabled' : 'Disabled'}</span>
                </span>
              </div>

              {/* Issues table */}
              <div className="border border-border rounded-md overflow-hidden">
                <table className="w-full text-sm">
                  <thead>
                    <tr className="bg-muted">
                      <th className="text-left px-3 py-1.5 font-medium text-xs uppercase tracking-wide text-muted-foreground"></th>
                      <th className="text-right px-3 py-1.5 font-medium text-xs uppercase tracking-wide text-muted-foreground">New</th>
                      <th className="text-right px-3 py-1.5 font-medium text-xs uppercase tracking-wide text-muted-foreground">Total</th>
                    </tr>
                  </thead>
                  <tbody className="divide-y divide-border">
                    <tr>
                      <td className="px-3 py-2">
                        <span className="inline-flex items-center gap-1.5">
                          <Hash className="h-3.5 w-3.5 text-amber-500" />
                          Suspect hashes
                        </span>
                      </td>
                      <td className="px-3 py-2 text-right font-medium">
                        {details.is_hash ? (
                          details.new_hash_suspect_count > 0 ? (
                            <span className="text-amber-500">{details.new_hash_suspect_count.toLocaleString()}</span>
                          ) : (
                            <span className="text-muted-foreground">0</span>
                          )
                        ) : (
                          <span className="text-muted-foreground">&mdash;</span>
                        )}
                      </td>
                      <td className="px-3 py-2 text-right font-medium">
                        {details.hash_suspect_count > 0 ? (
                          <span className="text-amber-500">{details.hash_suspect_count.toLocaleString()}</span>
                        ) : (
                          <span className="text-muted-foreground">{details.is_hash ? '0' : '\u2014'}</span>
                        )}
                      </td>
                    </tr>
                    <tr>
                      <td className="px-3 py-2">
                        <span className="inline-flex items-center gap-1.5">
                          <CircleX className="h-3.5 w-3.5 text-rose-500" />
                          Validation errors
                        </span>
                      </td>
                      <td className="px-3 py-2 text-right font-medium">
                        {details.is_val ? (
                          details.new_val_invalid_count > 0 ? (
                            <span className="text-rose-500">{details.new_val_invalid_count.toLocaleString()}</span>
                          ) : (
                            <span className="text-muted-foreground">0</span>
                          )
                        ) : (
                          <span className="text-muted-foreground">&mdash;</span>
                        )}
                      </td>
                      <td className="px-3 py-2 text-right font-medium">
                        {details.val_invalid_count > 0 ? (
                          <span className="text-rose-500">{details.val_invalid_count.toLocaleString()}</span>
                        ) : (
                          <span className="text-muted-foreground">{details.is_val ? '0' : '\u2014'}</span>
                        )}
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </CardContent>
          </Card>
        ) : null}
      </SheetContent>
    </Sheet>
  )
}
