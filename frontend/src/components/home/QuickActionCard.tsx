import type { ReactNode } from 'react'
import { Link } from 'react-router-dom'

interface QuickActionCardProps {
  icon: ReactNode
  title: string
  description: string
  to?: string
  href?: string
  onClick?: () => void
}

export function QuickActionCard({ icon, title, description, to, href, onClick }: QuickActionCardProps) {
  const className = "group flex flex-col gap-2 p-6 bg-card border border-border rounded-xl shadow-sm transition-all duration-150 hover:shadow-md hover:-translate-y-0.5 hover:border-primary/30 cursor-pointer"

  const content = (
    <>
      <div className="flex items-center gap-3">
        <div className="text-primary w-5 h-5 flex-shrink-0">
          {icon}
        </div>
        <h3 className="text-base font-semibold text-foreground">{title}</h3>
      </div>
      <p className="text-sm text-muted-foreground leading-relaxed">{description}</p>
    </>
  )

  if (to) {
    return (
      <Link to={to} className={className}>
        {content}
      </Link>
    )
  }

  if (href) {
    return (
      <a href={href} target="_blank" rel="noopener noreferrer" className={className}>
        {content}
      </a>
    )
  }

  return (
    <button onClick={onClick} className={className}>
      {content}
    </button>
  )
}
