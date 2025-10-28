import { useState, useEffect, useRef } from 'react'
import { Plus, CalendarCog } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { formatDateRelative } from '@/lib/dateUtils'
import type { RootWithScan } from '@/lib/types'

interface RootsTableProps {
  onAddRoot: () => void
  onScanClick: (rootId: number, isIncomplete: boolean) => void
  isScanning: boolean
}

const ITEMS_PER_PAGE = 25

export function RootsTable({ onAddRoot, onScanClick, isScanning }: RootsTableProps) {
  const [roots, setRoots] = useState<RootWithScan[]>([])
  const [loading, setLoading] = useState(true)
  const [currentPage, setCurrentPage] = useState(1)
  const [totalCount, setTotalCount] = useState(0)

  const loadRoots = async () => {
    try {
      setLoading(true)
      const response = await fetch('/api/roots/with-scans')
      if (!response.ok) throw new Error('Failed to load roots')

      const data: RootWithScan[] = await response.json()
      setRoots(data)
      setTotalCount(data.length)
    } catch (error) {
      console.error('Error loading roots:', error)
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadRoots()
  }, [])

  // Track previous scan state using a ref to detect completion
  const wasScanningRef = useRef(isScanning)

  useEffect(() => {
    // Detect scan completion (was scanning, now not scanning)
    if (wasScanningRef.current && !isScanning) {
      console.log('Scan completed, reloading roots data')
      // Give backend time to finish writing to database
      const timer = setTimeout(() => {
        loadRoots()
      }, 1500)

      wasScanningRef.current = isScanning
      return () => clearTimeout(timer)
    }

    // Update ref for next check
    wasScanningRef.current = isScanning
  }, [isScanning])

  // Pagination
  const startIndex = (currentPage - 1) * ITEMS_PER_PAGE
  const endIndex = Math.min(startIndex + ITEMS_PER_PAGE, totalCount)
  const paginatedRoots = roots.slice(startIndex, endIndex)

  if (loading) {
    return (
      <Card>
        <CardContent className="py-8">
          <div className="text-center text-muted-foreground">Loading roots...</div>
        </CardContent>
      </Card>
    )
  }

  return (
    <Card>
      <CardHeader>
        <div className="flex items-center justify-between">
          <CardTitle>Roots</CardTitle>
          <button
            onClick={onAddRoot}
            className="flex items-center gap-2 px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors text-sm font-medium"
          >
            <Plus className="h-4 w-4" />
            Add Root
          </button>
        </div>
      </CardHeader>
      <CardContent>
        {paginatedRoots.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            No roots found. Click "Add Root" to get started.
          </div>
        ) : (
          <div className="border border-border rounded-lg overflow-hidden">
            <table className="w-full">
              <tbody>
                {paginatedRoots.map((root) => {
                  const scanInfo = root.last_scan
                  const nonTerminalStates = ['Pending', 'Scanning', 'Sweeping', 'Analyzing']
                  const isIncomplete = scanInfo ? nonTerminalStates.includes(scanInfo.state) : false

                  let buttonText = 'Scan'
                  if (isIncomplete) {
                    buttonText = 'Resume'
                  }

                  // Determine if we should show the status line (Error, Stopped, or Incomplete)
                  let showStatusLine = false
                  let statusBadgeText = ''
                  let statusBadgeVariant: 'error' | 'warning' = 'error'
                  let statusMessage = ''

                  if (scanInfo) {
                    if (scanInfo.state === 'Error') {
                      showStatusLine = true
                      statusBadgeText = 'Error'
                      statusBadgeVariant = 'error'
                      statusMessage = scanInfo.error || 'Unknown error'
                    } else if (scanInfo.state === 'Stopped') {
                      showStatusLine = true
                      statusBadgeText = 'Stopped'
                      statusBadgeVariant = 'warning'
                      statusMessage = 'Stopped by user'
                    } else if (isIncomplete) {
                      showStatusLine = true
                      statusBadgeText = 'Incomplete'
                      statusBadgeVariant = 'warning'
                      statusMessage = 'This scan did not complete and can be resumed'
                    }
                  }

                  return (
                    <tr key={root.root_id} className="border-b border-border last:border-b-0">
                      <td className="p-0">
                        <div className="flex items-center gap-4 py-3 px-4">
                          {/* Scan Button */}
                          <div className="flex-shrink-0">
                            <button
                              onClick={() => onScanClick(root.root_id, isIncomplete)}
                              disabled={isScanning}
                              className="px-4 py-1.5 min-w-[80px] rounded-md bg-blue-600 text-white hover:bg-blue-700 disabled:bg-gray-400 disabled:cursor-not-allowed transition-colors text-sm font-medium"
                            >
                              {buttonText}
                            </button>
                          </div>

                          {/* Vertical Separator */}
                          <div className="w-px self-stretch bg-border" />

                          {/* Root Info (2-3 lines) */}
                          <div className="flex-1 flex flex-col gap-1.5 min-w-0">
                            {/* Line 1: Root path and last scan time */}
                            <div className="flex items-baseline gap-3 flex-wrap">
                              <span className="font-medium text-base">
                                {root.root_path}
                              </span>
                              {scanInfo && (
                                <span className="text-sm text-muted-foreground">
                                  Last Scan: {formatDateRelative(scanInfo.scan_time)}
                                </span>
                              )}
                            </div>

                            {/* Line 2: Status (optional - only if error, stopped, or incomplete) */}
                            {showStatusLine && (
                              <div className="flex items-center gap-2">
                                <Badge variant={statusBadgeVariant}>
                                  {statusBadgeText}
                                </Badge>
                                <span className="text-sm text-muted-foreground font-mono">
                                  {statusMessage}
                                </span>
                              </div>
                            )}

                            {/* Line 3: Schedule (always shown, but not implemented yet) */}
                            <div className="flex items-center gap-1.5">
                              <button
                                onClick={() => {
                                  // TODO: Implement scan scheduling
                                }}
                                className="inline-flex items-center justify-center rounded-md p-1.5 hover:bg-accent hover:text-accent-foreground transition-colors"
                                title="Schedule scan (coming soon)"
                              >
                                <CalendarCog className="h-5 w-5" />
                              </button>
                              <span className="text-sm text-muted-foreground">
                                No scheduled scans
                              </span>
                            </div>
                          </div>
                        </div>
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          </div>
        )}

        {/* Pagination */}
        {totalCount > ITEMS_PER_PAGE && (
          <div className="flex items-center justify-between mt-4">
            <div className="text-sm text-muted-foreground">
              Showing {startIndex + 1} - {endIndex} of {totalCount} roots
            </div>
            <div className="flex items-center gap-2">
              <button
                onClick={() => currentPage > 1 && setCurrentPage(p => p - 1)}
                disabled={currentPage === 1}
                className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
              >
                Previous
              </button>
              <button
                onClick={() => endIndex < totalCount && setCurrentPage(p => p + 1)}
                disabled={endIndex >= totalCount}
                className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
              >
                Next
              </button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  )
}
