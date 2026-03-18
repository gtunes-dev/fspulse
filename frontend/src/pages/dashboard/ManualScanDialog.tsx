import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { RootPicker } from '@/components/shared/RootPicker'
import { ScanOptionsFields } from '@/components/shared/ScanOptionsFields'
import { useTaskContext } from '@/contexts/TaskContext'
import { fetchQuery } from '@/lib/api'
import { Loader2 } from 'lucide-react'

interface Root {
  root_id: number
  root_path: string
}

interface ManualScanDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  preselectedRootId?: number
}

export function ManualScanDialog({ open, onOpenChange, preselectedRootId }: ManualScanDialogProps) {
  const { notifyTaskScheduled, isPaused } = useTaskContext()

  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [hashMode, setHashMode] = useState<string>('New or Changed')
  const [isVal, setIsVal] = useState<boolean>(true)

  const [loading, setLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Load roots and reset form when dialog opens
  useEffect(() => {
    if (open) {
      // Reset form to defaults
      setSelectedRootId(preselectedRootId ? preselectedRootId.toString() : '')
      setHashMode('New or Changed')
      setIsVal(true)
      setError(null)

      loadRoots()
    }
  }, [open, preselectedRootId])

  async function loadRoots() {
    try {
      setLoading(true)
      setError(null)

      const response = await fetchQuery('roots', {
        columns: [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
        ],
        filters: [],
        limit: 1000,
      })

      const rootData: Root[] = response.rows.map((row) => ({
        root_id: parseInt(row[0]),
        root_path: row[1],
      }))

      setRoots(rootData)

      // Auto-select if there's exactly one root
      if (rootData.length === 1) {
        setSelectedRootId(rootData[0].root_id.toString())
      }
    } catch (err) {
      console.error('Error loading roots:', err)
      setError(err instanceof Error ? err.message : 'Failed to load roots')
    } finally {
      setLoading(false)
    }
  }

  async function handleSubmit() {
    if (!selectedRootId) {
      setError('Please select a root')
      return
    }

    try {
      setSubmitting(true)
      setError(null)

      // Map UI hash mode to API value
      const mapHashMode = (mode: string): string => {
        if (mode === 'New or Changed') return 'New'
        return mode // 'All' and 'None' map directly
      }

      // Schedule the scan
      const response = await fetch('/api/tasks/scan', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({
          root_id: parseInt(selectedRootId),
          hash_mode: mapHashMode(hashMode),
          is_val: isVal,
        }),
      })

      if (!response.ok) {
        throw new Error(`Failed to schedule scan: ${response.statusText}`)
      }

      // Notify that a task was scheduled (triggers refresh in UpcomingTasksTable)
      notifyTaskScheduled()

      // WebSocket will receive state updates when backend starts the scan

      // Close dialog (form will reset when reopened)
      onOpenChange(false)
    } catch (err) {
      console.error('Error scheduling scan:', err)
      setError(err instanceof Error ? err.message : 'Failed to schedule scan')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[500px]">
        <DialogHeader>
          <DialogTitle>Scan Now</DialogTitle>
          <DialogDescription>
            Configure and start a manual scan of a root directory
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : roots.length === 0 ? (
          <div className="py-8 text-center">
            <p className="text-sm text-muted-foreground">
              No roots configured. Please add a root directory first.
            </p>
          </div>
        ) : (
          <div className="space-y-6">
            {/* Root Selection */}
            <div className="space-y-4">
              <label className="text-sm font-semibold">Root Directory</label>
              {preselectedRootId ? (
                <div className="rounded-md border border-input bg-muted px-3 py-2 text-sm font-mono text-muted-foreground">
                  {roots.find(r => r.root_id === preselectedRootId)?.root_path ?? `Root #${preselectedRootId}`}
                </div>
              ) : (
                <RootPicker
                  roots={roots}
                  value={selectedRootId}
                  onChange={setSelectedRootId}
                  placeholder="Select a root to scan"
                />
              )}
            </div>

            {/* Scan Options */}
            <ScanOptionsFields
              hashMode={hashMode}
              isVal={isVal}
              onHashModeChange={setHashMode}
              onIsValChange={setIsVal}
            />

            {/* Pause Warning */}
            {isPaused && (
              <div className="rounded-md bg-blue-50 dark:bg-blue-950/30 border border-blue-200 dark:border-blue-800 p-3">
                <p className="text-sm text-blue-600 dark:text-blue-400">
                  fsPulse is paused. This scan will be queued and will run when fsPulse is resumed.
                </p>
              </div>
            )}

            {/* Error Display */}
            {error && (
              <div className="rounded-md bg-red-50 p-3">
                <p className="text-sm text-red-600">{error}</p>
              </div>
            )}
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={submitting}
          >
            Cancel
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={loading || roots.length === 0 || !selectedRootId || submitting}
          >
            {submitting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Starting...
              </>
            ) : (
              isPaused ? 'Queue Scan' : 'Start Scan'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
