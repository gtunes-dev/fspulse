import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { useTaskContext } from '@/contexts/TaskContext'
import type { ScheduleWithRoot } from '@/lib/types'

interface RunScheduleDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  schedule: ScheduleWithRoot | null
}

export function RunScheduleDialog({
  open,
  onOpenChange,
  schedule,
}: RunScheduleDialogProps) {
  const { notifyTaskScheduled, isPaused } = useTaskContext()
  const [error, setError] = useState('')
  const [running, setRunning] = useState(false)

  const handleRun = async () => {
    if (!schedule) return

    setError('')
    setRunning(true)

    try {
      const response = await fetch('/api/tasks/scan', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          root_id: schedule.root_id,
          hash_mode: schedule.hash_mode,
          is_val: schedule.is_val,
        }),
      })

      if (!response.ok) {
        throw new Error(`Failed to start scan: ${response.statusText}`)
      }

      notifyTaskScheduled()
      handleOpenChange(false)
    } catch (err) {
      console.error('Error running scan:', err)
      setError(err instanceof Error ? err.message : 'Failed to start scan')
    } finally {
      setRunning(false)
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      setError('')
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Run Scan Now</DialogTitle>
          <DialogDescription>
            Start a scan using this schedule's settings?
          </DialogDescription>
        </DialogHeader>

        {schedule && (
          <div className="space-y-4 py-4">
            <div className="bg-muted rounded-md p-3 space-y-2">
              <div>
                <div className="text-sm font-medium mb-0.5">Schedule</div>
                <div className="text-sm">{schedule.schedule_name}</div>
              </div>
              <div>
                <div className="text-sm font-medium mb-0.5">Root</div>
                <div className="text-sm font-mono">{schedule.root_path}</div>
              </div>
              <div>
                <div className="text-sm font-medium mb-0.5">Options</div>
                <div className="text-sm text-muted-foreground">
                  Hash: {schedule.hash_mode} &middot; Validate: {schedule.is_val ? 'Yes' : 'No'}
                </div>
              </div>
            </div>

            {isPaused && (
              <div className="rounded-md bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-800 p-3">
                <p className="text-sm text-blue-600 dark:text-blue-400">
                  fsPulse is paused. This scan will be queued and will run when fsPulse is resumed.
                </p>
              </div>
            )}

            {error && (
              <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
                {error}
              </div>
            )}
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => handleOpenChange(false)}
            disabled={running}
          >
            Cancel
          </Button>
          <Button
            onClick={handleRun}
            disabled={running}
          >
            {running ? 'Starting...' : isPaused ? 'Queue Scan' : 'Run Scan'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
