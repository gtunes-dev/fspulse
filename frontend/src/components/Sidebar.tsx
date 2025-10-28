import { Home, FolderSearch, Lightbulb, Database, Settings } from 'lucide-react'
import { useState } from 'react'
import { NavLink } from 'react-router-dom'

export function Sidebar() {
  const [isExpanded, setIsExpanded] = useState(false)

  const mainNavItems = [
    { icon: Home, label: 'Home', to: '/', end: true },
    { icon: FolderSearch, label: 'Scan', to: '/scan', end: true },
    { icon: Lightbulb, label: 'Insights', to: '/insights/alerts', end: false },
    { icon: Database, label: 'Explore', to: '/explore/roots', end: false },
  ]

  const settingsItems = [
    { icon: Settings, label: 'Settings', to: '/settings' },
  ]

  return (
    <aside
      className="bg-muted border-r border-border transition-all duration-200 ease-in-out"
      style={{ width: isExpanded ? '200px' : '64px' }}
      onMouseEnter={() => setIsExpanded(true)}
      onMouseLeave={() => setIsExpanded(false)}
    >
      <nav className="flex flex-col gap-1 p-2">
        {mainNavItems.map((item) => {
          const Icon = item.icon
          return (
            <NavLink
              key={item.label}
              to={item.to}
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
        })}

        {/* Separator */}
        <div className="my-2 border-t border-border" />

        {settingsItems.map((item) => {
          const Icon = item.icon
          return (
            <NavLink
              key={item.label}
              to={item.to}
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
        })}
      </nav>
    </aside>
  )
}
