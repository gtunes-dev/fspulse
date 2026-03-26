import { Plus, Triangle, X, Minus } from 'lucide-react'
import type { ChangeKind } from '@/lib/pathUtils'

interface ChangeDotsProps {
  changeKind: ChangeKind
  isDir: boolean
  addCount?: number | null
  modifyCount?: number | null
  deleteCount?: number | null
  unchangedCount?: number | null
}

const ICON_BOX = 'inline-flex items-center justify-center w-4 h-4 flex-shrink-0'
const ICON_SIZE = 'h-3.5 w-3.5'

/**
 * Renders change-kind indicators for items in browse views.
 *
 * Files: a colored icon for their own change state.
 *
 * Directories: a fixed 2×2 grid showing which change types exist among
 * descendants. The folder icon itself carries the folder's own change
 * state color (handled by the parent component).
 * Grid positions:
 *   TL = added (green)    TR = modified (blue)
 *   BL = deleted (red)    BR = unchanged (zinc)
 * Each cell is saturated when that change type is present, or faintly
 * tinted when absent — so the grid is always the same shape.
 *
 * Counts are always accurate: changed folders have per-scan counts;
 * unchanged folders have derived counts (0 adds/mods/dels, unchanged =
 * total alive) computed at the data mapping layer.
 */
export function ChangeDots({ changeKind, isDir, addCount, modifyCount, deleteCount, unchangedCount }: ChangeDotsProps) {
  // Files: icon for own state
  if (!isDir) {
    const tooltip =
      changeKind === 'added' ? 'Added' :
      changeKind === 'modified' ? 'Modified' :
      changeKind === 'deleted' ? 'Deleted' :
      'Unchanged'
    const icon =
      changeKind === 'added' ? <Plus className={`${ICON_SIZE} text-green-500`} /> :
      changeKind === 'modified' ? <Triangle className={`${ICON_SIZE} text-blue-500`} fill="currentColor" /> :
      changeKind === 'deleted' ? <X className={`${ICON_SIZE} text-red-500`} /> :
      <Minus className={`${ICON_SIZE} text-foreground`} />
    return (
      <span className={ICON_BOX} title={tooltip}>
        {icon}
      </span>
    )
  }

  // Directories: 2×2 grid showing descendant change types.
  // Only descendant counts — the folder's own state is on its icon.
  const hasAdd = addCount != null && addCount > 0
  const hasMod = modifyCount != null && modifyCount > 0
  const hasDel = deleteCount != null && deleteCount > 0
  const hasUnch = unchangedCount != null && unchangedCount > 0

  const parts: string[] = []
  if (hasAdd) parts.push(`${addCount} added`)
  if (hasMod) parts.push(`${modifyCount} modified`)
  if (hasDel) parts.push(`${deleteCount} deleted`)
  if (hasUnch) parts.push(`${unchangedCount} unchanged`)
  const dirTooltip = parts.length > 0 ? parts.join(', ') : 'No descendants'

  return (
    <span className="inline-grid grid-cols-2 gap-[2px] w-4 h-4 flex-shrink-0 p-[1px] rounded-[3px] border border-zinc-500" title={dirTooltip}>
      <span className={`rounded-[1px] ${hasAdd ? 'bg-green-500' : 'bg-green-500/15'}`} />
      <span className={`rounded-[1px] ${hasMod ? 'bg-blue-500' : 'bg-blue-500/15'}`} />
      <span className={`rounded-[1px] ${hasDel ? 'bg-red-500' : 'bg-red-500/15'}`} />
      <span className={`rounded-[1px] ${hasUnch ? 'bg-zinc-400' : 'bg-zinc-400/15'}`} />
    </span>
  )
}
