import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'

interface ScanOptionsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onConfirm: (hashMode: string, validateMode: string) => void
}

export function ScanOptionsDialog({ open, onOpenChange, onConfirm }: ScanOptionsDialogProps) {
  const [hashMode, setHashMode] = useState('All')
  const [validateMode, setValidateMode] = useState('New or Changed')

  // Reset to safe defaults whenever dialog opens
  useEffect(() => {
    if (open) {
      setHashMode('All')
      setValidateMode('New or Changed')
    }
  }, [open])

  const handleConfirm = () => {
    onConfirm(hashMode, validateMode)
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>Scan Options</DialogTitle>
          <DialogDescription>
            Configure how this scan should process files
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-4">
          {/* Hash Mode */}
          <div className="space-y-3">
            <label className="text-sm font-semibold">Hash Files</label>
            <div className="space-y-2">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="hash-mode"
                  value="All"
                  checked={hashMode === 'All'}
                  onChange={(e) => setHashMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">All</span>
              </label>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="hash-mode"
                  value="New or Changed"
                  checked={hashMode === 'New or Changed'}
                  onChange={(e) => setHashMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">New or Changed</span>
              </label>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="hash-mode"
                  value="None"
                  checked={hashMode === 'None'}
                  onChange={(e) => setHashMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">None</span>
              </label>
            </div>
          </div>

          {/* Validate Mode */}
          <div className="space-y-3">
            <label className="text-sm font-semibold">Validate Files</label>
            <div className="space-y-2">
              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="validate-mode"
                  value="All"
                  checked={validateMode === 'All'}
                  onChange={(e) => setValidateMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">All</span>
              </label>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="validate-mode"
                  value="New or Changed"
                  checked={validateMode === 'New or Changed'}
                  onChange={(e) => setValidateMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">New or Changed</span>
              </label>

              <label className="flex items-center gap-2 cursor-pointer">
                <input
                  type="radio"
                  name="validate-mode"
                  value="None"
                  checked={validateMode === 'None'}
                  onChange={(e) => setValidateMode(e.target.value)}
                  className="w-4 h-4"
                />
                <span className="text-sm">None</span>
              </label>
            </div>
          </div>
        </div>

        <DialogFooter>
          <button
            onClick={() => onOpenChange(false)}
            className="px-4 py-2 rounded-md border border-border hover:bg-accent transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleConfirm}
            className="px-4 py-2 rounded-md bg-primary text-primary-foreground hover:bg-primary/90 transition-colors"
          >
            Start Scan
          </button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
