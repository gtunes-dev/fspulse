import { useState, useCallback, useRef, useEffect } from 'react'
import { BrowserRouter, Routes, Route, useLocation } from 'react-router-dom'
import { TaskProvider, useTaskContext } from './contexts/TaskContext'
import { ScrollContext } from './contexts/ScrollContext'
import { AppSidebar } from './components/layout/Sidebar'
import { SidebarProvider, SidebarInset } from './components/ui/sidebar'
import { HomePage } from './pages/home/HomePage'
import { RootsPage } from './pages/roots/RootsPage'
import { SchedulesPage } from './pages/schedules/SchedulesPage'
import { SettingsPage } from './pages/settings/SettingsPage'
import { ExplorePage } from './pages/explore/ExplorePage'
import { IntegrityPage } from './pages/integrity/IntegrityPage'
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

  if (backendConnected === null) {
    return null // First connection attempt in progress — don't flash anything
  }

  if (!backendConnected) {
    return <BackendUnavailablePage />
  }

  return (
    <SidebarProvider defaultOpen={true}>
      <AppSidebar />
      <SidebarInset ref={mainRef} className="overflow-auto p-6">
        <ScrollContext.Provider value={mainElement}>
        <Routes>
          <Route path="/" element={<HomePage />} />
          <Route path="/roots" element={<RootsPage />} />
          <Route path="/schedules" element={<SchedulesPage />} />
          <Route path="/settings" element={<SettingsPage />} />
          <Route path="/explore/*" element={<ExplorePage />} />
          <Route path="/integrity" element={<IntegrityPage />} />
          <Route path="/trends/*" element={null} />
          <Route path="/history" element={<HistoryPage />} />
          <Route path="/browse" element={null} />
        </Routes>
        <KeepAlivePage isActive={location.pathname === '/browse'}>
          {(active) => <BrowsePage isActive={active} />}
        </KeepAlivePage>
        <KeepAlivePage isActive={location.pathname.startsWith('/trends')}>
          {() => <TrendsPage />}
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
