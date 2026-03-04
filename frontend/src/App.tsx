import { useState, useCallback, useRef, useEffect } from 'react'
import { BrowserRouter, Routes, Route, useLocation } from 'react-router-dom'
import { TaskProvider, useTaskContext } from './contexts/TaskContext'
import { ScrollContext } from './contexts/ScrollContext'
import { AppSidebar } from './components/layout/Sidebar'
import { SidebarProvider, SidebarInset } from './components/ui/sidebar'
import { DashboardPage } from './pages/dashboard/DashboardPage'
import { SetupPage } from './pages/setup/SetupPage'
import { ExplorePage } from './pages/explore/ExplorePage'
import { AlertsPage } from './pages/alerts/AlertsPage'
import { TrendsPage } from './pages/trends/TrendsPage'
import { BrowsePage } from './pages/browse/BrowsePage'
import { HistoryPage } from './pages/history/HistoryPage'
import { KeepAlivePage } from './components/layout/KeepAlivePage'
import { BackendUnavailablePage } from './components/layout/BackendUnavailablePage'

function AppContent() {
  const { backendConnected } = useTaskContext()
  const location = useLocation()
  const [mainElement, setMainElement] = useState<HTMLElement | null>(null)
  const mainRef = useCallback((node: HTMLElement | null) => {
    setMainElement(node)
  }, [])

  // Scroll position save/restore across page navigations
  const scrollPositionsRef = useRef<Map<string, number>>(new Map())
  const previousPathnameRef = useRef(location.pathname)

  useEffect(() => {
    const prevPath = previousPathnameRef.current
    const newPath = location.pathname

    if (prevPath === newPath) return

    // Save scroll position for the page we're leaving
    if (mainElement) {
      scrollPositionsRef.current.set(prevPath, mainElement.scrollTop)
    }

    previousPathnameRef.current = newPath

    // Restore scroll position for the page we're entering
    requestAnimationFrame(() => {
      if (mainElement) {
        mainElement.scrollTop = scrollPositionsRef.current.get(newPath) ?? 0
      }
    })
  }, [location.pathname, mainElement])

  if (!backendConnected) {
    return <BackendUnavailablePage />
  }

  return (
    <SidebarProvider defaultOpen={true}>
      <AppSidebar />
      <SidebarInset ref={mainRef} className="overflow-auto p-6">
        <ScrollContext.Provider value={mainElement}>
        <Routes>
          <Route path="/" element={<DashboardPage />} />
          <Route path="/setup" element={<SetupPage />} />
          <Route path="/explore/*" element={<ExplorePage />} />
          <Route path="/alerts" element={<AlertsPage />} />
          <Route path="/trends/*" element={<TrendsPage />} />
          <Route path="/history" element={<HistoryPage />} />
          <Route path="/browse" element={null} />
        </Routes>
        <KeepAlivePage isActive={location.pathname === '/browse'}>
          {(active) => <BrowsePage isActive={active} />}
        </KeepAlivePage>
        </ScrollContext.Provider>
      </SidebarInset>
    </SidebarProvider>
  )
}

function App() {
  return (
    <BrowserRouter>
      <TaskProvider>
        <AppContent />
      </TaskProvider>
    </BrowserRouter>
  )
}

export default App
