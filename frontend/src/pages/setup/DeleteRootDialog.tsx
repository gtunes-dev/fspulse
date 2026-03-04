import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { deleteRoot } from '@/lib/api'

interface DeleteRootDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  rootId: number | null
  rootPath: string
  onDeleteSuccess?: () => void
}

export function DeleteRootDialog({
  open,
  onOpenChange,
  rootId,
  rootPath,
  onDeleteSuccess,
}: DeleteRootDialogProps) {
  const [acknowledged, setAcknowledged] = useState(false)
  const [error, setError] = useState('')
  const [deleting, setDeleting] = useState(false)

  const handleDelete = async () => {
    if (!rootId || !acknowledged) return

    setError('')
    setDeleting(true)

    try {
      await deleteRoot(rootId)

      // Call success callback if provided
      if (onDeleteSuccess) {
        onDeleteSuccess()
      }

      // Close dialog
      handleOpenChange(false)
    } catch (err) {
      console.error('Error deleting root:', err)
      setError(err instanceof Error ? err.message : 'Failed to delete root')
    } finally {
      setDeleting(false)
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      // Reset form when closing
      setAcknowledged(false)
      setError('')
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Delete Root</DialogTitle>
          <DialogDescription>
            This action cannot be undone. This will permanently delete the root and all associated data.
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="bg-muted rounded-md p-3">
            <div className="text-sm font-medium mb-1">Root Path</div>
            <div className="text-sm font-mono">{rootPath}</div>
          </div>

          <div className="bg-muted border border-border rounded-md p-3">
            <div className="text-sm font-semibold mb-2">Warning</div>
            <div className="text-sm mb-2">
              All data associated with this root will be deleted. This includes:
            </div>
            <ul className="text-sm list-disc list-inside space-y-1">
              <li>Scans</li>
              <li>Items</li>
              <li>Changes</li>
              <li>Alerts</li>
            </ul>
          </div>

          <div className="flex items-start gap-3 p-3 border border-border rounded-md">
            <input
              type="checkbox"
              id="acknowledge"
              checked={acknowledged}
              onChange={(e) => setAcknowledged(e.target.checked)}
              className="mt-0.5 cursor-pointer"
            />
            <label htmlFor="acknowledge" className="text-sm cursor-pointer">
              I understand that this action cannot be undone and will permanently delete all associated data
            </label>
          </div>

          {error && (
            <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
              {error}
            </div>
          )}
        </div>

        <DialogFooter>
          <button
            onClick={() => handleOpenChange(false)}
            className="px-4 py-2 rounded-md border border-border hover:bg-accent transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleDelete}
            disabled={!acknowledged || deleting}
            className="px-4 py-2 rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {deleting ? 'Deleting...' : 'Delete Root'}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
