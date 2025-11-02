import type { ReactNode } from 'react'

interface FilterToolbarProps {
  children: ReactNode
}

export function FilterToolbar({ children }: FilterToolbarProps) {
  return (
    <div className="mb-8">
      <div className="flex items-center gap-5 px-6 py-4 bg-background rounded-xl border-2 border-border/60 shadow-lg shadow-black/5 dark:shadow-black/20">
        {children}
      </div>
    </div>
  )
}
