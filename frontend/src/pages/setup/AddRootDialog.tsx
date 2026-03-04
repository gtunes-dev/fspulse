import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Input } from '@/components/ui/input'

interface AddRootDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSuccess?: () => void
}

export function AddRootDialog({ open, onOpenChange, onSuccess }: AddRootDialogProps) {
  const [path, setPath] = useState('')
  const [error, setError] = useState('')
  const [success, setSuccess] = useState('')
  const [submitting, setSubmitting] = useState(false)

  const handleSubmit = async () => {
    // Reset states
    setError('')
    setSuccess('')

    // Validate input
    if (!path.trim()) {
      setError('Please enter a path')
      return
    }

    setSubmitting(true)

    try {
      const response = await fetch('/api/roots', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: path.trim() }),
      })

      const data = await response.json()

      if (response.ok) {
        setSuccess(`Root added successfully: ${data.root_path}`)
        // Notify parent of success
        onSuccess?.()
        // Close modal after a short delay
        setTimeout(() => {
          onOpenChange(false)
          setPath('')
          setSuccess('')
        }, 1500)
      } else {
        setError(data.error || 'Failed to add root')
      }
    } catch (err) {
      console.error('Error adding root:', err)
      setError('Network error. Please try again.')
    } finally {
      setSubmitting(false)
    }
  }

  const handleKeyPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleSubmit()
    }
  }

  const handleOpenChange = (newOpen: boolean) => {
    if (!newOpen) {
      // Reset form when closing
      setPath('')
      setError('')
      setSuccess('')
    }
    onOpenChange(newOpen)
  }

  return (
    <Dialog open={open} onOpenChange={handleOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Add New Root</DialogTitle>
          <DialogDescription>
            Enter the absolute path to the directory you want to track
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          <div className="space-y-2">
            <label htmlFor="root-path" className="text-sm font-medium">
              Root Path
            </label>
            <Input
              id="root-path"
              type="text"
              value={path}
              onChange={(e) => setPath(e.target.value)}
              onKeyPress={handleKeyPress}
              placeholder="/absolute/path/to/directory"
              className="font-mono"
              autoFocus
            />
          </div>

          {error && (
            <div className="text-sm text-red-600 dark:text-red-400 bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-3">
              {error}
            </div>
          )}

          {success && (
            <div className="text-sm text-green-600 dark:text-green-400 bg-green-50 dark:bg-green-950/30 border border-green-200 dark:border-green-800 rounded-md p-3">
              {success}
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
            onClick={handleSubmit}
            disabled={submitting}
            className="px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
          >
            {submitting ? 'Adding...' : 'Add Root'}
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
