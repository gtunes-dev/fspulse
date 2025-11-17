import { useState, useEffect } from 'react'
import { ChevronRight, CirclePause, Pencil } from 'lucide-react'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { PauseScanDialog } from './PauseScanDialog'

export function ActiveScanCard() {
  const { activeScan, currentScanId, isPaused, pauseUntil, stopScan } = useScanManager()
  const [detailsExpanded, setDetailsExpanded] = useState(() => {
    // Load from localStorage
    return localStorage.getItem('fspulse.scan.details.expanded') === 'true'
  })
  const [stopping, setStopping] = useState(false)
  const [breadcrumbs, setBreadcrumbs] = useState<string[]>([])
  const [showPauseDialog, setShowPauseDialog] = useState(false)

  const currentScan = activeScan

  // Update breadcrumbs from scan state - these come from WebSocket
  useEffect(() => {
    if (currentScan && currentScan.completed_phases) {
      setBreadcrumbs(currentScan.completed_phases)
    } else {
      setBreadcrumbs([])
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [currentScan?.completed_phases])

  // Save expanded state to localStorage
  useEffect(() => {
    localStorage.setItem('fspulse.scan.details.expanded', detailsExpanded ? 'true' : 'false')
  }, [detailsExpanded])

  // Prepare scan details if there's an active scan
  let statusValue: string | undefined
  let phaseNames: string[]
  let phaseName: string | undefined
  let percentage: number | undefined
  let itemsText = ''
  let showPercentage = false
  let isPhase1 = false
  let showPauseButton = false
  let pauseButtonText = 'Pause'
  let pauseButtonDisabled = false
  let showStopButton = false
  let stopButtonText = 'Stop'
  let stopButtonDisabled = false
  let showThreadDetails = false

  if (currentScan) {
    statusValue = currentScan.status?.status || 'running'
    phaseNames = ['Scanning Files', 'Tombstoning Deletes', 'Analyzing']
    phaseName = phaseNames[currentScan.phase - 1] || 'Processing'
    percentage = currentScan.progress?.percentage || 0

    // Calculate items text based on phase
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

    // Determine pause and stop button display based on status
    if (statusValue === 'running') {
      showPauseButton = true
      pauseButtonText = 'Pause'
      pauseButtonDisabled = false
      showStopButton = true
      stopButtonText = 'Stop'
      stopButtonDisabled = false
    } else if (statusValue === 'pausing') {
      showPauseButton = true
      pauseButtonText = 'Pausing'
      pauseButtonDisabled = true
      showStopButton = true
      stopButtonText = 'Stop'
      stopButtonDisabled = true
    } else if (statusValue === 'stopping') {
      showPauseButton = true
      pauseButtonText = 'Pause'
      pauseButtonDisabled = true
      showStopButton = true
      stopButtonText = 'Stopping'
      stopButtonDisabled = true
    } else if (statusValue === 'stopped') {
      showPauseButton = false
      showStopButton = true
      stopButtonText = 'Stopped'
      stopButtonDisabled = true
    } else if (statusValue === 'completed') {
      showPauseButton = false
      // Check if we were stopping (race condition)
      if (stopping) {
        showStopButton = true
        stopButtonText = 'Completed'
        stopButtonDisabled = true
      } else {
        showStopButton = false
      }
    }

    showThreadDetails = currentScan.phase === 3 && currentScan.threads && currentScan.threads.length > 0
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

  return (
    <Card>
      <CardHeader>
        <CardTitle>Active Scan</CardTitle>
      </CardHeader>
      <CardContent>
        {/* Bordered content box - similar to table containers */}
        <div className="border border-border rounded-lg p-4">
          {!currentScan ? (
            // No active scan - show pause/unpause controls
            <div className="flex items-center min-h-[100px]">
              {/* Left half - Pause/Edit button */}
              <div className="flex-1 flex items-center justify-center">
                {isPaused ? (
                  <Button onClick={() => setShowPauseDialog(true)} size="lg">
                    <Pencil className="h-4 w-4 mr-2" />
                    Edit Pause
                  </Button>
                ) : (
                  <Button onClick={() => setShowPauseDialog(true)} size="lg">
                    <CirclePause className="h-4 w-4 mr-2" />
                    Pause Scanning
                  </Button>
                )}
              </div>

              {/* Vertical separator */}
              <div className="h-16 w-px bg-border" />

              {/* Right half - Status message */}
              <div className="flex-1 flex items-center justify-center">
                {isPaused ? (
                  <div className="text-center">
                    <div className="text-sm font-semibold mb-1">System Paused</div>
                    <p className="text-xs text-muted-foreground">
                      {pauseUntil === -1
                        ? 'Paused indefinitely'
                        : pauseUntil !== null
                        ? `Until ${new Date(pauseUntil * 1000).toLocaleString()}`
                        : 'Paused'}
                    </p>
                  </div>
                ) : (
                  <p className="text-sm text-muted-foreground">
                    No scan in progress
                  </p>
                )}
              </div>
            </div>
          ) : (
            <>
              {/* Header */}
              <div className="flex items-start justify-between mb-3">
                <div className="flex-1">
                  <div className="text-lg font-semibold">
                    Scanning: {currentScan.root_path}
                  </div>
                </div>
                <div className="flex gap-2">
                  {showPauseButton && (
                    <Button
                      onClick={() => setShowPauseDialog(true)}
                      disabled={pauseButtonDisabled}
                    >
                      {pauseButtonText}
                    </Button>
                  )}
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
                  {showPercentage && <span className="text-muted-foreground">{percentage?.toFixed(1)}%</span>}
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
            </>
          )}
        </div>
      </CardContent>

      {/* Pause Dialog */}
      <PauseScanDialog
        open={showPauseDialog}
        onOpenChange={setShowPauseDialog}
      />
    </Card>
  )
}
