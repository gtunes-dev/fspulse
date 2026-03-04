import { LayoutDashboard, FolderTree, TriangleAlert, TrendingUp, Clock, Database, Wrench } from 'lucide-react'
import { useState } from 'react'
import { NavLink, useLocation } from 'react-router-dom'

// Pages where root_id context is meaningful
const ROOT_SCOPED_PATHS = ['/browse', '/alerts', '/trends']

export function Sidebar() {
  const [isExpanded, setIsExpanded] = useState(false)
  const location = useLocation()

  // Read root_id from current URL to carry between root-scoped pages
  const currentRootId = new URLSearchParams(location.search).get('root_id')

  // Build destination URL, carrying root_id for root-scoped pages
  const buildTo = (basePath: string): string => {
    if (!currentRootId) return basePath
    // Only carry root_id if the destination is a root-scoped page
    const isRootScoped = ROOT_SCOPED_PATHS.some(p => basePath.startsWith(p))
    if (!isRootScoped) return basePath
    const separator = basePath.includes('?') ? '&' : '?'
    return `${basePath}${separator}root_id=${currentRootId}`
  }

  // Primary navigation: user goals — "why I opened the app"
  const primaryNavItems = [
    { icon: LayoutDashboard, label: 'Dashboard', to: '/', end: true },
    { icon: FolderTree, label: 'Browse', to: '/browse', end: true },
    { icon: TriangleAlert, label: 'Alerts', to: '/alerts', end: true },
    { icon: TrendingUp, label: 'Trends', to: '/trends/scan-trends', end: false },
  ]

  // Utility navigation: operational/investigative — "when I need to go deeper"
  const utilityNavItems = [
    { icon: Clock, label: 'History', to: '/history', end: true },
    { icon: Database, label: 'Data Explorer', to: '/explore/roots', end: false },
    { icon: Wrench, label: 'Setup', to: '/setup', end: false },
  ]

  const renderNavItems = (items: typeof primaryNavItems) =>
    items.map((item) => {
      const Icon = item.icon
      return (
        <NavLink
          key={item.label}
          to={buildTo(item.to)}
          end={item.end}
          className={({ isActive }) =>
            `flex items-center gap-3 rounded-lg px-3 py-2 transition-colors ${
              isActive
                ? 'bg-accent text-accent-foreground'
                : 'text-muted-foreground hover:bg-accent hover:text-accent-foreground'
            }`
          }
        >
          <Icon className="h-5 w-5 flex-shrink-0" />
          <span
            className="whitespace-nowrap overflow-hidden transition-all"
            style={{
              opacity: isExpanded ? 1 : 0,
              width: isExpanded ? 'auto' : 0,
            }}
          >
            {item.label}
          </span>
        </NavLink>
      )
    })

  return (
    <aside
      className="bg-muted border-r border-border transition-all duration-200 ease-in-out"
      style={{ width: isExpanded ? '200px' : '64px' }}
      onMouseEnter={() => setIsExpanded(true)}
      onMouseLeave={() => setIsExpanded(false)}
    >
      <nav className="flex flex-col gap-1 p-2">
        {renderNavItems(primaryNavItems)}

        {/* Separator between primary and utility navigation */}
        <div className="my-2 border-t border-border" />

        {renderNavItems(utilityNavItems)}
      </nav>
    </aside>
  )
}
