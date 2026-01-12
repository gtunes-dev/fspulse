import { useState, useMemo } from 'react'
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

interface PauseScanDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function PauseScanDialog({
  open,
  onOpenChange,
}: PauseScanDialogProps) {
  const { pauseTasks, unpauseTasks, isPaused, pauseUntil } = useTaskContext()
  const [error, setError] = useState('')
  const [pausing, setPausing] = useState(false)
  const [unpausing, setUnpausing] = useState(false)
  const [selectedDuration, setSelectedDuration] = useState<number | null>(null)

  // Calculate seconds until next midnight (12am) - memoized to stay stable across re-renders
  // Recalculate when dialog opens to ensure fresh calculation
  const secondsUntilTomorrow = useMemo(() => {
    const now = new Date()
    const tomorrow = new Date(now)
    tomorrow.setDate(tomorrow.getDate() + 1)
    tomorrow.setHours(0, 0, 0, 0)
    return Math.floor((tomorrow.getTime() - now.getTime()) / 1000)
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open])

  const durationOptions = [
    { label: '5 minutes', value: 300 },
    { label: '15 minutes', value: 900 },
    { label: '1 hour', value: 3600 },
    { label: '24 hours', value: 86400 },
    { label: 'Until tomorrow', value: secondsUntilTomorrow },
    { label: 'Until I unpause', value: -1 },
  ]

  const handlePause = async () => {
    if (selectedDuration === null) {
      setError('Please select a duration')
      return
    }

    setError('')
    setPausing(true)

    try {
      await pauseTasks(selectedDuration)

      // Close dialog on success
      handleOpenChange(false)
    } catch (err) {
      console.error('Error pausing:', err)
      setError(err instanceof Error ? err.message : 'Failed to pause')
    } finally {
      setPausing(false)
    }
  }

  const handleUnpause = async () => {
    setError('')
    setUnpausing(true)

    try {
      await unpauseTasks()

      // Close dialog on success
      handleOpenChange(false)
    } catch (err) {
      console.error('Error unpausing:', err)
      setError(err instanceof Error ? err.message : 'Failed to unpause')
    } finally {
      setUnpausing(false)
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      // Reset state when closing
      setError('')
      setSelectedDuration(null)
      setPausing(false)
      setUnpausing(false)
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>{isPaused ? 'Edit Pause' : 'Pause Scanning'}</DialogTitle>
          <DialogDescription>
            {isPaused ? (
              <>
                Currently paused{' '}
                {pauseUntil === -1
                  ? 'indefinitely'
                  : `until ${new Date(pauseUntil! * 1000).toLocaleString()}`}
                . You can update the duration or unpause now.
              </>
            ) : (
              'Scanning will be paused for the selected duration. Any in-progress scan will be stopped and can resume later.'
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <div className="text-sm font-medium mb-3">
              {isPaused ? 'Update pause duration:' : 'How long would you like to pause?'}
            </div>
            <div className="grid grid-cols-2 gap-2">
              {durationOptions.map((option) => (
                <button
                  key={option.value}
                  onClick={() => setSelectedDuration(option.value)}
                  className={`
                    px-4 py-3 rounded-md text-sm font-medium transition-colors
                    border-2
                    ${selectedDuration === option.value
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border hover:border-primary/50 hover:bg-muted'
                    }
                  `}
                >
                  {option.label}
                </button>
              ))}
            </div>
          </div>

          {error && (
            <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
              {error}
            </div>
          )}
        </div>

        {isPaused ? (
          <div className="flex items-center justify-between mt-4 pt-4 border-t">
            <Button
              onClick={handleUnpause}
              disabled={unpausing || pausing}
            >
              {unpausing ? 'Unpausing...' : 'Unpause Now'}
            </Button>
            <div className="flex gap-2">
              <Button
                variant="outline"
                onClick={() => handleOpenChange(false)}
                disabled={pausing || unpausing}
              >
                Cancel
              </Button>
              <Button
                onClick={handlePause}
                disabled={pausing || unpausing || selectedDuration === null}
              >
                {pausing ? 'Updating...' : 'Update Duration'}
              </Button>
            </div>
          </div>
        ) : (
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => handleOpenChange(false)}
              disabled={pausing}
            >
              Cancel
            </Button>
            <Button
              onClick={handlePause}
              disabled={pausing || selectedDuration === null}
            >
              {pausing ? 'Pausing...' : 'Pause'}
            </Button>
          </DialogFooter>
        )}
      </DialogContent>
    </Dialog>
  )
}
