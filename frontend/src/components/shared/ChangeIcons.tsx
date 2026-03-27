import { Plus, Triangle, X } from 'lucide-react'
import { formatCount } from '@/lib/formatUtils'

interface ChangeIconsProps {
  add: number | null
  modify: number | null
  del: number | null
}

export function ChangeIcons({ add, modify, del }: ChangeIconsProps) {
  if (!add && !modify && !del) return null

  return (
    <span className="inline-flex items-center gap-2.5 text-sm tabular-nums">
      {add ? (
        <span className="inline-flex items-center gap-1 text-green-500">
          <Plus className="h-3.5 w-3.5" />
          {formatCount(add)}
        </span>
      ) : null}
      {modify ? (
        <span className="inline-flex items-center gap-1 text-blue-500">
          <Triangle className="h-3 w-3" fill="currentColor" />
          {formatCount(modify)}
        </span>
      ) : null}
      {del ? (
        <span className="inline-flex items-center gap-1 text-red-500">
          <X className="h-3.5 w-3.5" />
          {formatCount(del)}
        </span>
      ) : null}
    </span>
  )
}
