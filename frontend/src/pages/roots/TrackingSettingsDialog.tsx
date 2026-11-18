import { useEffect, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { IgnorePatternsEditor } from '@/components/shared/IgnorePatternsEditor'
import { parsePatterns, updateObservation, type RootObservation } from '@/lib/observation'

interface TrackingSettingsDialogProps {
  observation: RootObservation | null
  open: boolean
  onOpenChange: (open: boolean) => void
  onSaved: () => void
}

/**
 * Per-root settings for how changes are found: whether the root is
 * watched live, and what is left out.
 */
export function TrackingSettingsDialog({ observation, open, onOpenChange, onSaved }: TrackingSettingsDialogProps) {
  const [watchEnabled, setWatchEnabled] = useState(true)
  const [patterns, setPatterns] = useState('')
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  // Load the root's current settings each time the dialog opens.
  useEffect(() => {
    if (open && observation) {
      setWatchEnabled(observation.watch_enabled)
      setPatterns(observation.ignore_patterns.join('\n'))
      setError('')
    }
  }, [open, observation])

  if (!observation) return null

  const before = new Set(observation.ignore_patterns)
  const added = parsePatterns(patterns).filter((pattern) => !before.has(pattern))

  const save = async () => {
    setSaving(true)
    setError('')
    try {
      await updateObservation(observation.root_id, {
        watch_enabled: watchEnabled,
        ignore_patterns: parsePatterns(patterns),
      })
      onSaved()
      onOpenChange(false)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to save settings')
    } finally {
      setSaving(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-xl">
        <DialogHeader>
          <DialogTitle>Change tracking</DialogTitle>
          <DialogDescription className="font-mono break-all">{observation.root_path}</DialogDescription>
        </DialogHeader>

        <div className="space-y-6 py-2">
          <div className="flex items-start justify-between gap-4">
            <div className="space-y-1">
              <label htmlFor="watch-enabled" className="text-sm font-medium">
                Watch for changes as they happen
              </label>
              <p className="text-xs text-muted-foreground">
                When off, changes are found only by the periodic full check, every few hours. Turn
                this off for network drives that do not report changes.
              </p>
            </div>
            <Switch id="watch-enabled" checked={watchEnabled} onCheckedChange={setWatchEnabled} />
          </div>

          <div className="space-y-2">
            <label htmlFor="ignore-patterns" className="text-sm font-medium">
              Ignore
            </label>
            <IgnorePatternsEditor id="ignore-patterns" value={patterns} onChange={setPatterns} />
            {added.length > 0 && (
              <p className="text-xs text-amber-600 dark:text-amber-400">
                Items already tracked that match {added.length === 1 ? 'the new pattern' : 'the new patterns'}{' '}
                will be shown as deleted from the next full check on. Their history is kept, and
                removing the pattern brings them back.
              </p>
            )}
          </div>

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
          <Button onClick={save} disabled={saving}>
            {saving ? 'Saving...' : 'Save'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
