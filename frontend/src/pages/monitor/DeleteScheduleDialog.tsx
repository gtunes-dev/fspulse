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

interface DeleteScheduleDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  scheduleId: number | null
  scheduleName: string
  onDeleteSuccess?: () => void
}

export function DeleteScheduleDialog({
  open,
  onOpenChange,
  scheduleId,
  scheduleName,
  onDeleteSuccess,
}: DeleteScheduleDialogProps) {
  const [error, setError] = useState('')
  const [deleting, setDeleting] = useState(false)

  const handleDelete = async () => {
    if (!scheduleId) return

    setError('')
    setDeleting(true)

    try {
      const response = await fetch(`/api/schedules/${scheduleId}`, {
        method: 'DELETE',
      })

      if (!response.ok) {
        throw new Error('Failed to delete schedule')
      }

      // Call success callback if provided
      if (onDeleteSuccess) {
        onDeleteSuccess()
      }

      // Close dialog
      handleOpenChange(false)
    } catch (err) {
      console.error('Error deleting schedule:', err)
      setError(err instanceof Error ? err.message : 'Failed to delete schedule')
    } finally {
      setDeleting(false)
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      // Reset error when closing
      setError('')
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Schedule</DialogTitle>
          <DialogDescription>
            Are you sure you want to delete this schedule?
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="bg-muted rounded-md p-3">
            <div className="text-sm font-medium mb-1">Schedule Name</div>
            <div className="text-sm">{scheduleName}</div>
          </div>

          <div className="text-sm text-muted-foreground">
            The schedule will be removed and no future scans will be triggered.
          </div>

          {error && (
            <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => handleOpenChange(false)}
            disabled={deleting}
          >
            Cancel
          </Button>
          <Button
            variant="destructive"
            onClick={handleDelete}
            disabled={deleting}
          >
            {deleting ? 'Deleting...' : 'Delete Schedule'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
