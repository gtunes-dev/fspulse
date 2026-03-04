import { useEffect, useState } from 'react'
import { NavLink, useLocation, useNavigate } from 'react-router-dom'
import {
  LayoutDashboard,
  FolderTree,
  TriangleAlert,
  TrendingUp,
  Clock,
  Database,
  Wrench,
  Moon,
  Sun,
  Power,
  Loader2,
  PanelLeftClose,
  PanelLeftOpen,
} from 'lucide-react'
import { useTheme } from '@/hooks/useTheme'
import { useTaskContext } from '@/contexts/TaskContext'
import { ShutdownDialog } from './ShutdownDialog'

// Pages where root_id context is meaningful
const ROOT_SCOPED_PATHS = ['/browse', '/alerts', '/trends']

function shortenPath(path: string, maxLength = 30): string {
  if (!path || path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return '.../' + parts.slice(-2).join('/')
}

export function Sidebar() {
  const [collapsed, setCollapsed] = useState<boolean>(() => {
    return localStorage.getItem('fspulse.sidebar.collapsed') === 'true'
  })
  const [showShutdownDialog, setShowShutdownDialog] = useState(false)

  const location = useLocation()
  const navigate = useNavigate()
  const { theme, toggleTheme } = useTheme()
  const { activeTask } = useTaskContext()

  // Persist collapse state
  useEffect(() => {
    localStorage.setItem('fspulse.sidebar.collapsed', collapsed ? 'true' : 'false')
  }, [collapsed])

  // Read root_id from current URL to carry between root-scoped pages
  const currentRootId = new URLSearchParams(location.search).get('root_id')

  // Build destination URL, carrying root_id for root-scoped pages
  const buildTo = (basePath: string): string => {
    if (!currentRootId) return basePath
    const isRootScoped = ROOT_SCOPED_PATHS.some(p => basePath.startsWith(p))
    if (!isRootScoped) return basePath
    const separator = basePath.includes('?') ? '&' : '?'
    return `${basePath}${separator}root_id=${currentRootId}`
  }

  // Task progress derived values
  const isRunning = activeTask !== null
  const isError = activeTask?.status === 'error'
  const headerText = activeTask
    ? (activeTask.target ? `${activeTask.action}: ${shortenPath(activeTask.target)}` : activeTask.action)
    : ''
  const phaseText = activeTask?.phase ?? ''
  const hasPercentage = activeTask?.progress_bar?.percentage !== null && activeTask?.progress_bar?.percentage !== undefined
  const percentage = hasPercentage ? Math.round(activeTask!.progress_bar!.percentage!) : 0

  // Primary navigation: user goals
  const primaryNavItems = [
    { icon: LayoutDashboard, label: 'Dashboard', to: '/', end: true },
    { icon: FolderTree, label: 'Browse', to: '/browse', end: true },
    { icon: TriangleAlert, label: 'Alerts', to: '/alerts', end: true },
    { icon: TrendingUp, label: 'Trends', to: '/trends/scan-trends', end: false },
  ]

  // Utility navigation: operational/investigative
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
            className="whitespace-nowrap overflow-hidden transition-all duration-200"
            style={{
              opacity: collapsed ? 0 : 1,
              width: collapsed ? 0 : 'auto',
            }}
          >
            {item.label}
          </span>
        </NavLink>
      )
    })

  return (
    <aside
      className="flex flex-col h-full bg-muted border-r border-border transition-all duration-200 ease-in-out"
      style={{ width: collapsed ? '64px' : '220px' }}
    >
      {/* Brand area */}
      <div className="flex items-center gap-3 px-3 py-3 border-b border-border flex-shrink-0">
        <img
          src="/favicon.svg"
          alt="FsPulse"
          className="h-6 w-6 flex-shrink-0"
        />
        <span
          className="text-lg font-semibold whitespace-nowrap overflow-hidden transition-all duration-200"
          style={{
            opacity: collapsed ? 0 : 1,
            width: collapsed ? 0 : 'auto',
          }}
        >
          FsPulse
        </span>
      </div>

      {/* Navigation */}
      <nav className="flex flex-col gap-1 p-2 flex-shrink-0">
        {renderNavItems(primaryNavItems)}
        <div className="my-2 border-t border-border" />
        {renderNavItems(utilityNavItems)}
      </nav>

      {/* Spacer */}
      <div className="flex-1" />

      {/* Task progress */}
      {isRunning && activeTask && (
        collapsed ? (
          <div className="flex justify-center py-2 flex-shrink-0">
            <button
              onClick={() => navigate('/')}
              className="rounded-md p-2 hover:bg-accent transition-colors"
              title={headerText}
            >
              <Loader2
                className={`h-5 w-5 animate-spin ${isError ? 'text-red-500' : 'text-primary'}`}
              />
            </button>
          </div>
        ) : (
          <div
            className="mx-2 mb-2 px-3 py-2 rounded-md border border-border bg-card cursor-pointer hover:bg-accent/50 transition-colors flex-shrink-0"
            onClick={() => navigate('/')}
          >
            <div className="text-xs font-medium truncate">
              {headerText}
            </div>
            <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground mt-0.5">
              {isError ? (
                <span className="text-red-600 dark:text-red-400 truncate">
                  Error: {activeTask.error_message}
                </span>
              ) : (
                <>
                  {phaseText && <span className="truncate">{phaseText}</span>}
                  {hasPercentage && (
                    <>
                      {phaseText && <span>&middot;</span>}
                      <div className="flex-1 h-[3px] bg-muted rounded-sm overflow-hidden">
                        <div
                          className="h-full bg-primary transition-all duration-300 rounded-sm"
                          style={{ width: `${percentage}%` }}
                        />
                      </div>
                      <span className="font-medium text-foreground min-w-[28px] text-right">
                        {percentage}%
                      </span>
                    </>
                  )}
                </>
              )}
            </div>
          </div>
        )
      )}

      {/* Controls */}
      {collapsed ? (
        <div className="flex flex-col items-center gap-1 py-2 border-t border-border flex-shrink-0">
          <button
            onClick={toggleTheme}
            className="rounded-md p-2 hover:bg-accent transition-colors"
            title={`Switch to ${theme === 'light' ? 'dark' : 'light'} mode`}
          >
            {theme === 'light' ? (
              <Moon className="h-5 w-5" />
            ) : (
              <Sun className="h-5 w-5" />
            )}
          </button>
          <button
            onClick={() => setShowShutdownDialog(true)}
            className="rounded-md p-2 hover:bg-accent transition-colors text-muted-foreground hover:text-destructive"
            title="Shut down server"
          >
            <Power className="h-5 w-5" />
          </button>
          <button
            onClick={() => setCollapsed(false)}
            className="rounded-md p-2 hover:bg-accent transition-colors"
            title="Expand sidebar"
            aria-label="Expand sidebar"
          >
            <PanelLeftOpen className="h-5 w-5" />
          </button>
        </div>
      ) : (
        <div className="flex items-center justify-between px-3 py-2 border-t border-border flex-shrink-0">
          <div className="flex items-center gap-1">
            <button
              onClick={toggleTheme}
              className="rounded-md p-2 hover:bg-accent transition-colors"
              title={`Switch to ${theme === 'light' ? 'dark' : 'light'} mode`}
            >
              {theme === 'light' ? (
                <Moon className="h-5 w-5" />
              ) : (
                <Sun className="h-5 w-5" />
              )}
            </button>
            <button
              onClick={() => setShowShutdownDialog(true)}
              className="rounded-md p-2 hover:bg-accent transition-colors text-muted-foreground hover:text-destructive"
              title="Shut down server"
            >
              <Power className="h-5 w-5" />
            </button>
          </div>
          <button
            onClick={() => setCollapsed(true)}
            className="rounded-md p-2 hover:bg-accent transition-colors"
            title="Collapse sidebar"
            aria-label="Collapse sidebar"
          >
            <PanelLeftClose className="h-5 w-5" />
          </button>
        </div>
      )}

      <ShutdownDialog
        open={showShutdownDialog}
        onOpenChange={setShowShutdownDialog}
      />
    </aside>
  )
}
