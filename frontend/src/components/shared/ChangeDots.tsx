import type { ChangeKind } from '@/lib/pathUtils'
import { formatCount } from '@/lib/formatUtils'

interface ChangeDotsProps {
  isDir: boolean
  /** How many items, this one and (for a folder) those beneath it, each kind of change happened to. See `changeCounts`. */
  addCount?: number | null
  modifyCount?: number | null
  deleteCount?: number | null
  unchangedCount?: number | null
  /** Change types currently hidden by the filter — suppressed in the grid */
  hiddenKinds?: Set<ChangeKind>
}

/**
 * The change square: what happened at or beneath a row inside the
 * selected time range. The same square is drawn for files and folders.
 *
 * A fixed 2×2 grid:
 *   TL = added (green)    TR = modified (blue)
 *   BL = deleted (red)    BR = unchanged (zinc)
 * Each cell is saturated when that kind of change happened at least
 * once, or faintly tinted when it did not — so the grid is always the
 * same shape and a kind is always in the same place.
 *
 * For a file, a cell is lit if that happened to the file: a file added
 * and later modified lights two cells. The unchanged cell is lit when
 * nothing happened to it.
 *
 * For a folder, a cell is lit if that happened to the folder itself or
 * to anything beneath it. Unchanged items are not counted: the cell
 * only says the folder exists at the end of the range and so may hold
 * some.
 */
export function ChangeDots({ isDir, addCount, modifyCount, deleteCount, unchangedCount, hiddenKinds }: ChangeDotsProps) {
  const hasAdd = addCount != null && addCount > 0 && !hiddenKinds?.has('added')
  const hasMod = modifyCount != null && modifyCount > 0 && !hiddenKinds?.has('modified')
  const hasDel = deleteCount != null && deleteCount > 0 && !hiddenKinds?.has('deleted')
  const hasUnch = unchangedCount != null && unchangedCount > 0 && !hiddenKinds?.has('unchanged')

  const parts: string[] = []
  if (isDir) {
    if (hasAdd) parts.push(`${formatCount(addCount)} added`)
    if (hasMod) parts.push(`${formatCount(modifyCount)} modified`)
    if (hasDel) parts.push(`${formatCount(deleteCount)} deleted`)
    // Unchanged items are not counted, only known to be possible.
    if (hasUnch) parts.push('unchanged items')
  } else {
    if (hasAdd) parts.push('added')
    if (hasMod) parts.push('modified')
    if (hasDel) parts.push('deleted')
    if (hasUnch) parts.push('unchanged')
  }
  const joined = parts.join(', ')
  const tooltip = joined === '' ? 'Nothing to show' : joined.charAt(0).toUpperCase() + joined.slice(1)

  return (
    <span className="inline-grid grid-cols-2 gap-[2px] w-4 h-4 flex-shrink-0 p-[1px] rounded-[3px] border border-zinc-500" title={tooltip}>
      <span className={`rounded-[1px] ${hasAdd ? 'bg-green-500' : 'bg-green-500/15'}`} />
      <span className={`rounded-[1px] ${hasMod ? 'bg-blue-500' : 'bg-blue-500/15'}`} />
      <span className={`rounded-[1px] ${hasDel ? 'bg-red-500' : 'bg-red-500/15'}`} />
      <span className={`rounded-[1px] ${hasUnch ? 'bg-zinc-400' : 'bg-zinc-400/15'}`} />
    </span>
  )
}
