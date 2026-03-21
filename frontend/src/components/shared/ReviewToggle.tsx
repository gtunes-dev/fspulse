import { CircleHelp, CircleCheckBig } from 'lucide-react'
import { cn } from '@/lib/utils'

interface ReviewToggleProps {
  reviewed: boolean
  onToggle: () => void
  disabled?: boolean
  size?: 'default' | 'sm'
}

export function ReviewToggle({ reviewed, onToggle, disabled = false, size = 'default' }: ReviewToggleProps) {
  const iconClass = size === 'sm' ? 'h-[18px] w-[18px]' : 'h-5 w-5'
  const padClass = size === 'sm' ? 'px-1 py-0.5' : 'px-1.5 py-1'

  return (
    <button
      className={cn(
        "inline-flex items-center rounded-md border-2 border-border overflow-hidden transition-opacity",
        disabled && "opacity-50 pointer-events-none",
      )}
      onClick={onToggle}
      disabled={disabled}
      title={reviewed ? "Mark as not reviewed" : "Mark as reviewed"}
    >
      {/* Not reviewed side */}
      <span
        className={cn(
          "flex items-center justify-center transition-all duration-150",
          padClass,
          !reviewed
            ? "bg-primary text-primary-foreground"
            : "text-muted-foreground/60"
        )}
      >
        <CircleHelp className={iconClass} />
      </span>
      {/* Reviewed side */}
      <span
        className={cn(
          "flex items-center justify-center transition-all duration-150",
          padClass,
          reviewed
            ? "bg-primary text-primary-foreground"
            : "text-muted-foreground/60"
        )}
      >
        <CircleCheckBig className={iconClass} />
      </span>
    </button>
  )
}
