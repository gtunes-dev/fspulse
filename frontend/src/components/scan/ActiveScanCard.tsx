import { useState, useEffect } from 'react'
import { ChevronRight } from 'lucide-react'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'

export function ActiveScanCard() {
  const { activeScans, currentScanId, stopScan } = useScanManager()
  const [detailsExpanded, setDetailsExpanded] = useState(() => {
    // Load from localStorage
    return localStorage.getItem('fspulse.scan.details.expanded') === 'true'
  })
  const [stopping, setStopping] = useState(false)
  const [breadcrumbs, setBreadcrumbs] = useState<string[]>([])

  const currentScan = currentScanId ? activeScans.get(currentScanId) : null

  // Update breadcrumbs from scan state - these come from WebSocket
  useEffect(() => {
    if (currentScan && currentScan.completed_phases) {
      setBreadcrumbs(currentScan.completed_phases)
    } else {
      setBreadcrumbs([])
    }
  }, [currentScan?.completed_phases])

  // Save expanded state to localStorage
  useEffect(() => {
    localStorage.setItem('fspulse.scan.details.expanded', detailsExpanded ? 'true' : 'false')
  }, [detailsExpanded])

  if (!currentScan) {
    return (
      <Card>
        <CardContent className="flex items-center justify-center py-8">
          <h3 className="text-lg font-medium text-muted-foreground">No Active Scan</h3>
        </CardContent>
      </Card>
    )
  }

  const statusValue = currentScan.status?.status || 'running'
  const phaseNames = ['Scanning Files', 'Tombstoning Deletes', 'Analyzing']
  const phaseName = phaseNames[currentScan.phase - 1] || 'Processing'
  const percentage = currentScan.progress?.percentage || 0

  // Calculate items text based on phase
  let itemsText = ''
  let showPercentage = false
  let isPhase1 = false

  if (currentScan.phase === 3) {
    itemsText = `${currentScan.progress.current.toLocaleString()} / ${currentScan.progress.total.toLocaleString()} files`
    showPercentage = true
  } else if (currentScan.phase === 1) {
    // Phase 1: show file and directory counts
    if (currentScan.scanning_counts) {
      const files = currentScan.scanning_counts.files.toLocaleString()
      const dirs = currentScan.scanning_counts.directories.toLocaleString()
      itemsText = `${files} files in ${dirs} directories`
    } else {
      itemsText = 'Scanning files...'
    }
    isPhase1 = true
  } else if (currentScan.phase === 2) {
    // Phase 2: also show file and directory counts
    if (currentScan.scanning_counts) {
      const files = currentScan.scanning_counts.files.toLocaleString()
      const dirs = currentScan.scanning_counts.directories.toLocaleString()
      itemsText = `${files} files in ${dirs} directories`
    } else {
      itemsText = 'Processing...'
    }
  }

  const handleStop = async () => {
    if (!currentScanId) return
    setStopping(true)
    try {
      await stopScan(currentScanId)
    } catch (error) {
      console.error('Failed to stop scan:', error)
      alert('Failed to stop scan. Please try again.')
    }
    // Note: don't set stopping to false - let the WebSocket update handle it
  }

  // Determine stop button display based on status (matching old implementation)
  let showStopButton = false
  let stopButtonText = 'Stop'
  let stopButtonDisabled = false

  if (statusValue === 'running') {
    showStopButton = true
    stopButtonText = 'Stop'
    stopButtonDisabled = false
  } else if (statusValue === 'cancelling') {
    showStopButton = true
    stopButtonText = 'Stopping'
    stopButtonDisabled = true
  } else if (statusValue === 'stopped') {
    showStopButton = true
    stopButtonText = 'Stopped'
    stopButtonDisabled = true
  } else if (statusValue === 'completed') {
    // Check if we were stopping (race condition)
    if (stopping) {
      showStopButton = true
      stopButtonText = 'Completed'
      stopButtonDisabled = true
    } else {
      showStopButton = false
    }
  }

  const showThreadDetails = currentScan.phase === 3 && currentScan.threads && currentScan.threads.length > 0

  return (
    <Card>
      <CardContent className="pt-6">
        {/* Header */}
        <div className="flex items-start justify-between mb-3">
          <div className="flex-1">
            <div className="text-lg font-semibold">
              Scanning: {currentScan.root_path}
            </div>
          </div>
          {showStopButton && (
            <button
              onClick={handleStop}
              disabled={stopButtonDisabled}
              className="px-4 py-2 rounded-md text-sm font-medium bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
            >
              {stopButtonText}
            </button>
          )}
        </div>

        {/* Breadcrumbs - Completed Phases */}
        {breadcrumbs.length > 0 && (
          <div className="mb-3 space-y-1">
            {breadcrumbs.map((crumb, idx) => (
              <div key={idx} className="text-sm text-green-600 dark:text-green-400">
                ✓ {crumb}
              </div>
            ))}
          </div>
        )}

        {/* Error Message */}
        {currentScan.error_message && statusValue === 'error' && (
          <div className="text-sm text-red-600 dark:text-red-400 font-mono mb-3">
            {currentScan.error_message}
          </div>
        )}

        {/* Progress Section */}
        <div className="mb-3">
          <div className="flex items-center justify-between text-sm mb-2">
            <span className="font-medium">Phase {currentScan.phase} of 3: {phaseName}</span>
            {showPercentage && <span className="text-muted-foreground">{percentage.toFixed(1)}%</span>}
          </div>
          {showPercentage && (
            <div className="w-full h-2 bg-muted rounded-sm overflow-hidden mb-2">
              <div
                className="h-full bg-primary transition-all duration-300"
                style={{ width: `${percentage}%` }}
              />
            </div>
          )}
          <div className={`text-sm ${isPhase1 ? 'text-primary' : ''}`}>
            {itemsText}
          </div>
        </div>

        {/* Thread Details Toggle (Phase 3 only) */}
        {showThreadDetails && (
          <>
            <button
              onClick={() => setDetailsExpanded(!detailsExpanded)}
              className="flex items-center gap-2 text-sm font-medium hover:text-foreground transition-colors w-full py-2"
            >
              <ChevronRight
                className={`h-4 w-4 transition-transform ${detailsExpanded ? 'rotate-90' : ''}`}
              />
              <span>Thread Details</span>
            </button>

            {detailsExpanded && (
              <div className="mt-2 border-t border-border pt-2">
                {currentScan.threads && currentScan.threads.length > 0 ? (
                  currentScan.threads.map((thread) => {
                    let operation = 'idle'
                    let filePath = thread.current_file || '-'

                    if (thread.current_file) {
                      if (thread.current_file.startsWith('Hashing:')) {
                        operation = 'hashing'
                        filePath = thread.current_file.substring(9).trim()
                      } else if (thread.current_file.startsWith('Validating:')) {
                        operation = 'validating'
                        filePath = thread.current_file.substring(12).trim()
                      } else if (thread.status === 'active') {
                        operation = 'scanning'
                      }
                    }

                    const statusLabels: Record<string, string> = {
                      hashing: 'HASHING',
                      validating: 'VALIDATING',
                      scanning: 'SCANNING',
                      idle: 'IDLE',
                    }

                    // Map operations to semantic badge variants
                    const badgeVariants: Record<string, 'info' | 'info-alternate' | 'success' | 'secondary'> = {
                      hashing: 'info',
                      validating: 'info-alternate',
                      scanning: 'success',
                      idle: 'secondary',
                    }

                    return (
                      <div key={thread.thread_index} className="flex items-center gap-3 text-sm py-2 border-b border-border last:border-b-0">
                        <Badge
                          variant={badgeVariants[operation]}
                          className="min-w-[90px] justify-center font-bold text-xs"
                        >
                          {statusLabels[operation]}
                        </Badge>
                        <span className="text-muted-foreground truncate text-sm" title={filePath}>
                          {filePath}
                        </span>
                      </div>
                    )
                  })
                ) : (
                  <div className="py-3 text-sm text-muted-foreground italic text-center">
                    Waiting for thread activity...
                  </div>
                )}
              </div>
            )}
          </>
        )}
      </CardContent>
    </Card>
  )
}
