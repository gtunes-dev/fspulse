/**
 * How a root is being kept up to date: its settings and the watcher's
 * current view of it. Mirrors `GET /api/roots/observation`.
 */
export type WatchState = 'watching' | 'degraded' | 'unwatched' | 'disabled' | 'offline'

export type WalkBlock = 'empty_root' | 'mass_delete'

export interface RootObservation {
  root_id: number
  root_path: string
  watch_enabled: boolean
  ignore_patterns: string[]
  last_walk_started_at: number | null
  last_walk_completed_at: number | null
  /**
   * The ignore patterns changed and no full check has completed since.
   * Nothing new is recorded for ignored paths, but items tracked before
   * the change may not be relabelled yet.
   */
  ignore_pending: boolean
  /** Null until the watcher has looked at the root. */
  watch: WatchState | null
  watch_detail: string | null
  walk_block: WalkBlock | null
  walk_block_detail: string | null
}

export async function fetchObservation(): Promise<RootObservation[]> {
  const response = await fetch('/api/roots/observation')
  if (!response.ok) throw new Error('Failed to load change tracking status')
  return (await response.json()) as RootObservation[]
}

export async function updateObservation(
  rootId: number,
  settings: {
    watch_enabled?: boolean
    ignore_patterns?: string[]
    /** Queue a full check after saving, so already-tracked items are relabelled now. */
    run_check?: boolean
  }
): Promise<void> {
  const response = await fetch(`/api/roots/${rootId}/observation`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(settings),
  })
  if (!response.ok) {
    const data = await response.json().catch(() => null)
    throw new Error(data?.error ?? 'Failed to save settings')
  }
}

/**
 * Ask for the whole root to be compared with what is recorded.
 * `confirmDeletions` releases a check that refused to record a large
 * number of deletions.
 */
export async function requestChangeCheck(rootId: number, confirmDeletions = false): Promise<void> {
  const response = await fetch(`/api/roots/${rootId}/checkpoint`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ confirm_deletions: confirmDeletions }),
  })
  if (!response.ok) {
    const data = await response.json().catch(() => null)
    throw new Error(data?.error ?? 'Failed to start the check')
  }
}

export interface TrackingSummary {
  /** Short state, always shown. */
  label: string
  /** Badge style when the state needs attention; none when all is well. */
  attention: 'warning' | 'error' | null
  /** One sentence on what the state means for the user. */
  explanation: string | null
}

/** The watch state in the user's terms. */
export function describeTracking(observation: RootObservation): TrackingSummary {
  switch (observation.watch) {
    case 'watching':
      return { label: 'Watching', attention: null, explanation: null }
    case 'degraded':
      return {
        label: 'Partly watching',
        attention: 'warning',
        explanation:
          'The system could not watch every folder, so some changes are only found by the periodic full check.',
      }
    case 'unwatched':
      return {
        label: 'Not watching',
        attention: 'warning',
        explanation: observation.watch_detail
          ? `Watching could not be started: ${observation.watch_detail}. Changes are found by the periodic full check.`
          : 'Watching could not be started. Changes are found by the periodic full check.',
      }
    case 'disabled':
      return {
        label: 'Watching off',
        attention: null,
        explanation: 'Changes are found by the periodic full check.',
      }
    case 'offline':
      return {
        label: 'Folder not found',
        attention: 'error',
        explanation: 'The folder is missing. If it is on an external or network drive, connect it.',
      }
    default:
      return { label: 'Starting', attention: null, explanation: null }
  }
}

/**
 * When the whole root was last compared with what is recorded.
 * `checkRunning` is true while a change check of this root is the
 * active task.
 */
export function describeLastCheck(
  observation: RootObservation,
  checkRunning: boolean,
  timeAgo: (seconds: number) => string
): string {
  if (checkRunning) return 'Full check in progress'
  if (observation.last_walk_completed_at) {
    return `Checked in full ${timeAgo(observation.last_walk_completed_at).toLowerCase()}`
  }
  // Started is recorded when a check begins and completed is cleared,
  // so a start without a completion is a check that did not finish.
  if (observation.last_walk_started_at) return 'Last full check did not finish'
  return 'Not yet checked in full'
}

/** A pattern worth offering, with what it leaves out. */
export interface IgnoreSuggestion {
  pattern: string
  description: string
}

/**
 * Common noise. Offered as suggestions; never applied unless the user
 * picks them.
 */
export const IGNORE_SUGGESTIONS: IgnoreSuggestion[] = [
  { pattern: '.DS_Store', description: 'macOS folder view settings' },
  { pattern: '.git', description: 'Git repository internals' },
  { pattern: 'node_modules', description: 'Installed JavaScript packages' },
  { pattern: '__pycache__', description: 'Compiled Python files' },
  { pattern: '.Trash', description: 'The macOS trash' },
  { pattern: '.cache', description: 'Application caches' },
  { pattern: 'Library/Caches', description: 'macOS application caches (home folders)' },
  { pattern: 'Library/Logs', description: 'macOS application logs (home folders)' },
  { pattern: '*.tmp', description: 'Temporary files' },
  { pattern: 'Thumbs.db', description: 'Windows thumbnail caches' },
]

/** One pattern per line; blank lines and repeats dropped. */
export function parsePatterns(text: string): string[] {
  const seen = new Set<string>()
  for (const line of text.split('\n')) {
    const pattern = line.trim()
    if (pattern) seen.add(pattern)
  }
  return [...seen]
}
