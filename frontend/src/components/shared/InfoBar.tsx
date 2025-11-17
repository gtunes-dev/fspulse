import type { ReactNode } from 'react'

interface InfoBarProps {
  variant: 'info' | 'warning' | 'success'
  icon?: React.ComponentType<{ className?: string }>
  children: ReactNode
}

export function InfoBar({ variant, icon: Icon, children }: InfoBarProps) {
  const iconColors = {
    info: 'text-blue-500',
    warning: 'text-purple-500',
    success: 'text-green-500',
  }

  return (
    <div className="flex items-center gap-3 px-4 py-3 rounded-lg border border-border bg-muted/30">
      {Icon && <Icon className={`h-5 w-5 flex-shrink-0 ${iconColors[variant]}`} />}
      <div className="text-sm text-muted-foreground flex-1">{children}</div>
    </div>
  )
}
