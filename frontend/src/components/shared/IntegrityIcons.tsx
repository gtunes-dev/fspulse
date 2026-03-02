import { AlertTriangle, CircleX } from 'lucide-react'
import type { HashState, ValState } from '@/lib/pathUtils'

interface IntegrityIconsProps {
  hashState?: HashState | null
  valState?: ValState | null
}

/**
 * Renders small warning icons next to items that have integrity problems.
 *
 * - Suspect hash: amber warning triangle
 * - Invalid validation: rose circle-X
 *
 * Only shown for problem states. Normal states (unknown, valid, no_validator)
 * produce no visual output, keeping the tree clean.
 */
export function IntegrityIcons({ hashState, valState }: IntegrityIconsProps) {
  const showSuspect = hashState === 'suspect'
  const showInvalid = valState === 'invalid'

  if (!showSuspect && !showInvalid) return null

  return (
    <span className="inline-flex items-center gap-0.5 flex-shrink-0">
      {showSuspect && (
        <span title="Suspect hash"><AlertTriangle className="h-3.5 w-3.5 text-amber-500" /></span>
      )}
      {showInvalid && (
        <span title="Invalid"><CircleX className="h-3.5 w-3.5 text-rose-500" /></span>
      )}
    </span>
  )
}
