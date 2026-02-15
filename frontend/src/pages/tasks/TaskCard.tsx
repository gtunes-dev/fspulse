import { useState, useEffect } from 'react'
import { ChevronRight, CirclePause, Play, Lightbulb } from 'lucide-react'
import { useTaskContext } from '@/contexts/TaskContext'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { InfoBar } from '@/components/shared/InfoBar'
import { ManualScanDialog } from './ManualScanDialog'
import { PauseDialog } from './PauseDialog'

export function TaskCard() {
  const { activeTask, currentTaskId, isExclusive, isPaused, stopTask } = useTaskContext()
  const [detailsExpanded, setDetailsExpanded] = useState(() => {
    return localStorage.getItem('fspulse.task.details.expanded') === 'true'
  })
  const [stopping, setStopping] = useState(false)
  const [showManualScanDialog, setShowManualScanDialog] = useState(false)
  const [showPauseDialog, setShowPauseDialog] = useState(false)

  useEffect(() => {
    localStorage.setItem('fspulse.task.details.expanded', detailsExpanded ? 'true' : 'false')
  }, [detailsExpanded])

  const handleStop = async () => {
    if (!currentTaskId) return
    setStopping(true)
    try {
      await stopTask(currentTaskId)
    } catch (error) {
      console.error('Failed to stop task:', error)
      alert('Failed to stop task. Please try again.')
    }
  }

  // Derive display state from activeTask
  const status = activeTask?.status ?? 'running'
  const isStoppable = activeTask?.is_stoppable ?? false
  const isPausable = activeTask?.is_pausable ?? true
  const showStopButton = isStoppable && ((activeTask && status !== 'completed') || (stopping && status === 'completed'))
  const stopButtonDisabled = status === 'pausing' || status === 'stopping' || status === 'stopped' || status === 'completed'
  const stopButtonText = status === 'stopping' ? 'Stopping' : status === 'stopped' ? 'Stopped' : status === 'completed' ? 'Completed' : 'Stop'
  const showThreadDetails = activeTask && activeTask.thread_states.length > 0
  const hasPercentage = activeTask?.progress_bar?.percentage !== null && activeTask?.progress_bar?.percentage !== undefined

  // Pause button label adapts based on whether the running task is pausable
  const getPauseButtonLabel = () => {
    if (isPaused) return 'Edit Pause'
    return 'Pause'
  }

  return (
    <>
      <Card>
        <CardContent className="pt-6">
          {/* Action Bar */}
          <div className="flex items-center gap-3 mb-4">
            <Button size="lg" onClick={() => setShowManualScanDialog(true)} disabled={isExclusive}>
              <Play className="h-4 w-4 mr-2" />
              Manual Scan
            </Button>
            <Button
              size="lg"
              variant="secondary"
              onClick={() => setShowPauseDialog(true)}
              disabled={isExclusive || !!(activeTask && (status === 'pausing' || status === 'stopping'))}
              className={isPaused ? 'text-purple-600 dark:text-purple-400' : ''}
            >
              <CirclePause className="h-4 w-4 mr-2" />
              {getPauseButtonLabel()}
            </Button>
          </div>

          {/* Pause status message when paused with non-pausable active task */}
          {isPaused && activeTask && !isPausable && status === 'running' && (
            <div className="mb-4">
              <InfoBar variant="info" icon={CirclePause}>
                Pause will take effect after current task completes
              </InfoBar>
            </div>
          )}

          {/* Content Area */}
          <div className="border border-border rounded-lg p-4">
            {!activeTask ? (
              <div className="space-y-4">
                <p className="text-sm text-muted-foreground text-center py-4">
                  No task in progress
                </p>
                <InfoBar variant="info" icon={Lightbulb}>
                  Configure recurring scans on the <a href="/monitor" className="underline hover:text-primary">Monitor</a> page
                </InfoBar>
              </div>
            ) : (
              <>
                {/* Header */}
                <div className="flex items-start justify-between mb-3">
                  <div className="flex-1">
                    <div className="text-lg font-semibold">
                      {activeTask.target ? `${activeTask.action}: ${activeTask.target}` : activeTask.action}
                    </div>
                  </div>
                  <div className="flex gap-2">
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

                {/* Breadcrumbs */}
                {activeTask.breadcrumbs.length > 0 && (
                  <div className="mb-3 space-y-1">
                    {activeTask.breadcrumbs.map((crumb, idx) => (
                      <div key={idx} className="text-sm text-green-600 dark:text-green-400">
                        ✓ {crumb}
                      </div>
                    ))}
                  </div>
                )}

                {/* Error Message */}
                {activeTask.error_message && status === 'error' && (
                  <div className="text-sm text-red-600 dark:text-red-400 font-mono mb-3">
                    {activeTask.error_message}
                  </div>
                )}

                {/* Progress Section */}
                <div className="mb-3">
                  {activeTask.phase && (
                    <div className="flex items-center justify-between text-sm mb-2">
                      <span className="font-medium">{activeTask.phase}</span>
                      {hasPercentage && (
                        <span className="text-muted-foreground">
                          {activeTask.progress_bar!.percentage!.toFixed(1)}%
                        </span>
                      )}
                    </div>
                  )}
                  {activeTask.progress_bar && hasPercentage && (
                    <div className="w-full h-2 bg-muted rounded-sm overflow-hidden mb-2">
                      <div
                        className="h-full bg-primary transition-all duration-300"
                        style={{ width: `${activeTask.progress_bar.percentage}%` }}
                      />
                    </div>
                  )}
                  {activeTask.progress_bar && !hasPercentage && (
                    <div className="w-full h-2 bg-primary/30 rounded-sm overflow-hidden mb-2 relative">
                      <div
                        className="absolute inset-0 bg-gradient-to-r from-transparent via-primary to-transparent"
                        style={{ animation: 'progress-shimmer 4s ease-in-out infinite' }}
                      />
                    </div>
                  )}
                  {activeTask.progress_bar?.message && (
                    <div className={`text-sm ${!hasPercentage ? 'text-primary' : ''}`}>
                      {activeTask.progress_bar.message}
                    </div>
                  )}
                </div>

                {/* Thread Details */}
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
                        {activeTask.thread_states.map((thread, idx) => (
                          <div key={idx} className="flex items-center gap-3 text-sm py-2 border-b border-border last:border-b-0">
                            <Badge
                              variant={thread.status_style as 'info' | 'info-alternate' | 'success' | 'secondary'}
                              className="min-w-[90px] justify-center font-bold text-xs"
                            >
                              {thread.status}
                            </Badge>
                            <span className="text-muted-foreground truncate text-sm" title={thread.detail ?? '-'}>
                              {thread.detail ?? '-'}
                            </span>
                          </div>
                        ))}
                      </div>
                    )}
                  </>
                )}
              </>
            )}
          </div>
        </CardContent>
      </Card>

      {/* Dialogs */}
      <ManualScanDialog open={showManualScanDialog} onOpenChange={setShowManualScanDialog} />
      <PauseDialog open={showPauseDialog} onOpenChange={setShowPauseDialog} />
    </>
  )
}
