import type { ChangeKind } from '@/lib/pathUtils'

interface ChangeDotsProps {
  changeKind: ChangeKind
  isDir: boolean
  addCount?: number | null
  modifyCount?: number | null
  deleteCount?: number | null
  unchangedCount?: number | null
}

// Single dot styling (files and unchanged directories)
const DOT = 'w-[7px] h-[7px] rounded-full'
const DOT_BOX = 'relative inline-block w-4 h-4 flex-shrink-0'

/**
 * Renders change-kind indicators for items in browse views.
 *
 * Files: single colored dot for their own change state.
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
  // Files: single dot for own state
  if (!isDir) {
    const color =
      changeKind === 'added' ? 'bg-green-500' :
      changeKind === 'modified' ? 'bg-blue-500' :
      changeKind === 'deleted' ? 'bg-red-500' :
      'bg-zinc-400'
    return (
      <span className={DOT_BOX}>
        <span className={`absolute left-[4.5px] top-[4.5px] ${DOT} ${color}`} />
      </span>
    )
  }

  // Directories: 2×2 grid showing descendant change types.
  // Only descendant counts — the folder's own state is on its icon.
  const hasAdd = addCount != null && addCount > 0
  const hasMod = modifyCount != null && modifyCount > 0
  const hasDel = deleteCount != null && deleteCount > 0
  const hasUnch = unchangedCount != null && unchangedCount > 0

  return (
    <span className="inline-grid grid-cols-2 gap-[2px] w-4 h-4 flex-shrink-0 p-[1px] rounded-[3px] border border-zinc-500">
      <span className={`rounded-[1px] ${hasAdd ? 'bg-green-500' : 'bg-green-500/15'}`} />
      <span className={`rounded-[1px] ${hasMod ? 'bg-blue-500' : 'bg-blue-500/15'}`} />
      <span className={`rounded-[1px] ${hasDel ? 'bg-red-500' : 'bg-red-500/15'}`} />
      <span className={`rounded-[1px] ${hasUnch ? 'bg-zinc-400' : 'bg-zinc-400/15'}`} />
    </span>
  )
}
