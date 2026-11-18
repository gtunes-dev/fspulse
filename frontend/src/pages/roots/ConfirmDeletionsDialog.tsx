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
import { requestChangeCheck, type RootObservation } from '@/lib/observation'

interface ConfirmDeletionsDialogProps {
  observation: RootObservation | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onConfirmed: () => void
}

/**
 * Shown when a full check refused to record a large number of
 * deletions. Recording them is the user's call: the same picture is
 * produced by an unmounted drive.
 */
export function ConfirmDeletionsDialog({ observation, open, onOpenChange, onConfirmed }: ConfirmDeletionsDialogProps) {
  const [working, setWorking] = useState(false)
  const [error, setError] = useState('')

  if (!observation) return null

  const confirm = async () => {
    setWorking(true)
    setError('')
    try {
      await requestChangeCheck(observation.root_id, true)
      onConfirmed()
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to start the check')
    } finally {
      setWorking(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Record these items as deleted?</DialogTitle>
          <DialogDescription className="font-mono break-all">{observation.root_path}</DialogDescription>
        </DialogHeader>

        <div className="space-y-3 py-2 text-sm">
          {observation.walk_block_detail && <p>{observation.walk_block_detail}</p>}
          <p className="text-muted-foreground">
            A drive that is not connected looks the same as a folder whose contents were deleted, so
            nothing has been recorded as deleted yet. If the files really are gone, confirm and the
            root will be checked again and the deletions recorded. If a drive is disconnected,
            cancel and reconnect it.
          </p>
          {error && (
            <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button variant="destructive" onClick={confirm} disabled={working}>
            {working ? 'Starting...' : 'They were deleted'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
