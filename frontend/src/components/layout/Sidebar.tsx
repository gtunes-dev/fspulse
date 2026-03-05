import { useState } from 'react'
import { NavLink, useLocation, useNavigate } from 'react-router-dom'
import {
  Home,
  FolderTree,
  TriangleAlert,
  TrendingUp,
  Clock,
  HardDrive,
  Calendar,
  Database,
  Wrench,
  Moon,
  Sun,
  Power,
  Loader2,
} from 'lucide-react'
import { useTheme } from '@/hooks/useTheme'
import { useTaskContext } from '@/contexts/TaskContext'
import {
  Sidebar as SidebarRoot,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarHeader,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarRail,
  SidebarSeparator,
  SidebarTrigger,
  useSidebar,
} from '@/components/ui/sidebar'
import { ShutdownDialog } from './ShutdownDialog'

// Pages where root_id context is meaningful
const ROOT_SCOPED_PATHS = ['/browse', '/alerts', '/trends', '/schedules', '/history']

function shortenPath(path: string, maxLength = 30): string {
  if (!path || path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return '.../' + parts.slice(-2).join('/')
}

export function AppSidebar() {
  const [showShutdownDialog, setShowShutdownDialog] = useState(false)

  const location = useLocation()
  const navigate = useNavigate()
  const { theme, toggleTheme } = useTheme()
  const { activeTask } = useTaskContext()
  const { state } = useSidebar()

  const collapsed = state === 'collapsed'

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

  // Navigation items
  const primaryNavItems = [
    { icon: Home, label: 'Home', to: '/', end: true },
    { icon: FolderTree, label: 'Browse', to: '/browse', end: true },
    { icon: TriangleAlert, label: 'Alerts', to: '/alerts', end: true },
    { icon: TrendingUp, label: 'Trends', to: '/trends/scan-trends', end: false },
  ]

  const utilityNavItems = [
    { icon: Clock, label: 'History', to: '/history', end: true },
    { icon: HardDrive, label: 'Roots', to: '/roots', end: true },
    { icon: Calendar, label: 'Schedules', to: '/schedules', end: true },
    { icon: Database, label: 'Data Explorer', to: '/explore/roots', end: false },
    { icon: Wrench, label: 'Settings', to: '/settings', end: true },
  ]

  const isNavActive = (item: typeof primaryNavItems[0]): boolean => {
    const path = location.pathname
    return item.end ? path === item.to : path.startsWith(item.to)
  }

  const renderNavItems = (items: typeof primaryNavItems) => (
    <SidebarMenu>
      {items.map((item) => {
        const Icon = item.icon
        return (
          <SidebarMenuItem key={item.label}>
            <SidebarMenuButton
              asChild
              tooltip={item.label}
              isActive={isNavActive(item)}
            >
              <NavLink to={buildTo(item.to)}>
                <Icon />
                <span>{item.label}</span>
              </NavLink>
            </SidebarMenuButton>
          </SidebarMenuItem>
        )
      })}
    </SidebarMenu>
  )

  return (
    <SidebarRoot collapsible="icon">
      {/* Brand area */}
      <SidebarHeader className="border-b border-sidebar-border">
        <div className="flex items-center gap-3 px-1 py-1">
          <svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" className="h-6 w-6 flex-shrink-0" aria-hidden="true">
            <path d="M20 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4l2 3h10a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2Z" />
            <polyline points="5 14 8 14 10 10 13 17 15 12 17 14 19 14" />
          </svg>
          {!collapsed && (
            <span className="text-lg font-semibold whitespace-nowrap">
              fsPulse
            </span>
          )}
        </div>
      </SidebarHeader>

      {/* Navigation */}
      <SidebarContent>
        <SidebarGroup>
          {renderNavItems(primaryNavItems)}
        </SidebarGroup>
        <SidebarSeparator />
        <SidebarGroup>
          {renderNavItems(utilityNavItems)}
        </SidebarGroup>
      </SidebarContent>

      {/* Footer: task progress + controls */}
      <SidebarFooter>
        {/* Task progress */}
        {isRunning && activeTask && (
          collapsed ? (
            <div className="flex justify-center">
              <button
                onClick={() => navigate('/')}
                className="rounded-md p-2 hover:bg-sidebar-accent transition-colors"
                title={headerText}
              >
                <Loader2
                  className={`h-5 w-5 animate-spin ${isError ? 'text-red-500' : 'text-primary'}`}
                />
              </button>
            </div>
          ) : (
            <div
              className="px-3 py-2 rounded-md border border-sidebar-border bg-sidebar cursor-pointer hover:bg-sidebar-accent/50 transition-colors"
              onClick={() => navigate('/')}
            >
              <div className="text-xs font-medium truncate">
                {headerText}
              </div>
              <div className="flex items-center gap-1.5 text-[11px] text-sidebar-foreground/60 mt-0.5">
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
                        <div className="flex-1 h-[3px] bg-sidebar-accent rounded-sm overflow-hidden">
                          <div
                            className="h-full bg-primary transition-all duration-300 rounded-sm"
                            style={{ width: `${percentage}%` }}
                          />
                        </div>
                        <span className="font-medium text-sidebar-foreground min-w-[28px] text-right">
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
          <div className="flex flex-col items-center gap-1">
            <button
              onClick={toggleTheme}
              className="rounded-md p-2 hover:bg-sidebar-accent transition-colors"
              title={`Switch to ${theme === 'light' ? 'dark' : 'light'} mode`}
            >
              {theme === 'light' ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
            </button>
            <button
              onClick={() => setShowShutdownDialog(true)}
              className="rounded-md p-2 hover:bg-sidebar-accent transition-colors text-sidebar-foreground/60 hover:text-destructive"
              title="Shut down server"
            >
              <Power className="h-4 w-4" />
            </button>
            <SidebarTrigger className="h-8 w-8" />
          </div>
        ) : (
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1">
              <button
                onClick={toggleTheme}
                className="rounded-md p-2 hover:bg-sidebar-accent transition-colors"
                title={`Switch to ${theme === 'light' ? 'dark' : 'light'} mode`}
              >
                {theme === 'light' ? <Moon className="h-4 w-4" /> : <Sun className="h-4 w-4" />}
              </button>
              <button
                onClick={() => setShowShutdownDialog(true)}
                className="rounded-md p-2 hover:bg-sidebar-accent transition-colors text-sidebar-foreground/60 hover:text-destructive"
                title="Shut down server"
              >
                <Power className="h-4 w-4" />
              </button>
            </div>
            <SidebarTrigger />
          </div>
        )}
      </SidebarFooter>

      <SidebarRail />

      <ShutdownDialog
        open={showShutdownDialog}
        onOpenChange={setShowShutdownDialog}
      />
    </SidebarRoot>
  )
}
