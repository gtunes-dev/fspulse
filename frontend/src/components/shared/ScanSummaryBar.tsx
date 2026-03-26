import { useState, useEffect, useRef } from 'react'
import { Files, Folder, HardDrive, Info } from 'lucide-react'
import { fetchQuery } from '@/lib/api'
import type { ColumnSpec } from '@/lib/types'
import { formatFileSizeCompact } from '@/lib/formatUtils'
import { formatDateTimeShort } from '@/lib/dateUtils'
import { ChangeIcons } from '@/components/shared/ChangeIcons'
import { ScanDetailSheet } from '@/components/shared/ScanDetailSheet'

interface ScanSummaryBarProps {
  scanId: number
}

interface ScanSummary {
  scan_id: number
  started_at: number
  file_count: number
  folder_count: number
  total_size: number | null
  add_count: number
  modify_count: number
  delete_count: number
}

const SCAN_COLUMNS: ColumnSpec[] = [
  { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
  { name: 'started_at', visible: true, sort_direction: 'none', position: 1 },
  { name: 'file_count', visible: true, sort_direction: 'none', position: 2 },
  { name: 'folder_count', visible: true, sort_direction: 'none', position: 3 },
  { name: 'total_size', visible: true, sort_direction: 'none', position: 4 },
  { name: 'add_count', visible: true, sort_direction: 'none', position: 5 },
  { name: 'modify_count', visible: true, sort_direction: 'none', position: 6 },
  { name: 'delete_count', visible: true, sort_direction: 'none', position: 7 },
]

export function ScanSummaryBar({ scanId }: ScanSummaryBarProps) {
  const [summary, setSummary] = useState<ScanSummary | null>(null)
  const [sheetOpen, setSheetOpen] = useState(false)
  const fetchIdRef = useRef(0)

  useEffect(() => {
    const currentId = ++fetchIdRef.current

    async function load() {
      try {
        const response = await fetchQuery('scans', {
          columns: SCAN_COLUMNS,
          filters: [{ column: 'scan_id', value: scanId.toString() }],
          limit: 1,
        })

        if (currentId !== fetchIdRef.current) return
        if (response.rows.length === 0) return

        const row = response.rows[0]
        setSummary({
          scan_id: parseInt(row[0]),
          started_at: parseInt(row[1]),
          file_count: parseInt(row[2]) || 0,
          folder_count: parseInt(row[3]) || 0,
          total_size: row[4] && row[4] !== '-' ? parseInt(row[4]) : null,
          add_count: parseInt(row[5]) || 0,
          modify_count: parseInt(row[6]) || 0,
          delete_count: parseInt(row[7]) || 0,
        })
      } catch {
        if (currentId !== fetchIdRef.current) return
      }
    }

    load()
  }, [scanId])

  if (!summary) return null

  const hasChanges = summary.add_count > 0 || summary.modify_count > 0 || summary.delete_count > 0

  return (
    <>
      <button
        className="flex items-center gap-5 px-4 py-2 border-b border-border bg-muted/30 text-left hover:bg-accent/50 transition-colors cursor-pointer w-full shrink-0"
        onClick={() => setSheetOpen(true)}
        title="Click for full scan details"
      >
        <span className="text-sm font-medium text-muted-foreground">
          Scan #{summary.scan_id} &mdash; {formatDateTimeShort(summary.started_at)}
        </span>

        <span className="inline-flex items-center gap-1.5 text-sm text-muted-foreground">
          <Files className="h-4 w-4" />
          {summary.file_count.toLocaleString()} files
        </span>

        <span className="inline-flex items-center gap-1.5 text-sm text-muted-foreground">
          <Folder className="h-4 w-4" />
          {summary.folder_count.toLocaleString()} folders
        </span>

        {summary.total_size !== null && (
          <span className="inline-flex items-center gap-1.5 text-sm text-muted-foreground">
            <HardDrive className="h-4 w-4" />
            {formatFileSizeCompact(summary.total_size)}
          </span>
        )}

        <span className="text-sm">
          {hasChanges ? (
            <ChangeIcons add={summary.add_count} modify={summary.modify_count} del={summary.delete_count} />
          ) : (
            <span className="text-muted-foreground">no changes</span>
          )}
        </span>

        <span className="ml-auto">
          <Info className="h-4 w-4 text-muted-foreground" />
        </span>
      </button>

      <ScanDetailSheet
        scanId={scanId}
        open={sheetOpen}
        onOpenChange={setSheetOpen}
      />
    </>
  )
}
