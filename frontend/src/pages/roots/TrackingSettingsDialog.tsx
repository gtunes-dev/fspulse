import { useEffect, useRef, useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Checkbox } from '@/components/ui/checkbox'
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
  // On by default: someone changing what is ignored expects to see it
  // take effect. Turning it off leaves the relabelling of items that
  // are already tracked to the next scheduled full check.
  const [runCheck, setRunCheck] = useState(true)
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState('')

  // Load the root's settings once per opening, not every time the page
  // refreshes its status. The Roots page re-reads every root's status
  // every 30 seconds, which hands this dialog a new `observation`
  // object; reloading from it would throw away what is being typed.
  const loadedRootRef = useRef<number | null>(null)
  useEffect(() => {
    if (!open) {
      loadedRootRef.current = null
      return
    }
    if (observation && loadedRootRef.current !== observation.root_id) {
      loadedRootRef.current = observation.root_id
      setWatchEnabled(observation.watch_enabled)
      setPatterns(observation.ignore_patterns.join('\n'))
      setRunCheck(true)
      setError('')
    }
  }, [open, observation])

  if (!observation) return null

  const before = new Set(observation.ignore_patterns)
  const current = parsePatterns(patterns)
  const added = current.filter((pattern) => !before.has(pattern))
  const removed = observation.ignore_patterns.filter((pattern) => !current.includes(pattern))
  const patternsChanged = added.length > 0 || removed.length > 0

  const save = async () => {
    setSaving(true)
    setError('')
    try {
      await updateObservation(observation.root_id, {
        watch_enabled: watchEnabled,
        ignore_patterns: current,
        run_check: patternsChanged && runCheck,
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
            {patternsChanged && (
              <div className="space-y-2 rounded-md border border-border p-3">
                <p className="text-xs text-muted-foreground">
                  The change applies to anything new as soon as you save.
                  {added.length > 0 &&
                    ' Items already tracked that match will stop being tracked: they are not deleted, their history is kept, and Browse can show them with "Show ignored items".'}
                  {removed.length > 0 &&
                    ' Items that no longer match will be tracked again if they are still there.'}{' '}
                  Items already tracked are updated by a full check.
                </p>
                <label className="flex items-center gap-2 text-sm cursor-pointer">
                  <Checkbox checked={runCheck} onCheckedChange={(checked) => setRunCheck(checked === true)} />
                  Run a full check now
                </label>
                {!runCheck && (
                  <p className="text-xs text-muted-foreground">
                    Without it, items already tracked are updated at the next scheduled full check.
                  </p>
                )}
              </div>
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
