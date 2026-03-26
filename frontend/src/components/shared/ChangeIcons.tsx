import { Plus, Triangle, Minus } from 'lucide-react'

interface ChangeIconsProps {
  add: number | null
  modify: number | null
  del: number | null
}

export function ChangeIcons({ add, modify, del }: ChangeIconsProps) {
  if (!add && !modify && !del) return null

  return (
    <span className="inline-flex items-center gap-2.5 text-sm">
      {add ? (
        <span className="inline-flex items-center gap-1 text-green-500">
          <Plus className="h-3.5 w-3.5" />
          {add}
        </span>
      ) : null}
      {modify ? (
        <span className="inline-flex items-center gap-1 text-blue-500">
          <Triangle className="h-3 w-3" />
          {modify}
        </span>
      ) : null}
      {del ? (
        <span className="inline-flex items-center gap-1 text-red-500">
          <Minus className="h-3.5 w-3.5" />
          {del}
        </span>
      ) : null}
    </span>
  )
}
