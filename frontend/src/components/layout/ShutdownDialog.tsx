import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface ShutdownDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
}

export function ShutdownDialog({ open, onOpenChange }: ShutdownDialogProps) {
  const [shuttingDown, setShuttingDown] = useState(false)
  const [error, setError] = useState('')

  const handleShutdown = async () => {
    setError('')
    setShuttingDown(true)

    try {
      const response = await fetch('/api/server/shutdown', { method: 'POST' })

      if (!response.ok) {
        throw new Error(`Shutdown request failed: ${response.statusText}`)
      }

      // Success — close dialog; the BackendUnavailablePage will take over
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to request shutdown')
      setShuttingDown(false)
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (shuttingDown) return
    if (!newOpen) {
      setError('')
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Shut Down</DialogTitle>
        </DialogHeader>

        <DialogDescription className="pt-2">
          Shut down the FsPulse server? Running tasks will be stopped and resumed when the server is restarted.
        </DialogDescription>

        {error && (
          <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
            {error}
          </div>
        )}

        <DialogFooter>
          <button
            onClick={() => handleOpenChange(false)}
            disabled={shuttingDown}
            className="px-4 py-2 rounded-md border border-border hover:bg-accent transition-colors disabled:opacity-50 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
          <button
            onClick={handleShutdown}
            disabled={shuttingDown}
            className="px-4 py-2 rounded-md bg-destructive text-destructive-foreground hover:bg-destructive/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {shuttingDown ? 'Shutting Down...' : 'Shut Down'}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
