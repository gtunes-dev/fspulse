import { useState, useCallback } from 'react'
import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { TaskProvider, useTaskContext } from './contexts/TaskContext'
import { ScrollContext } from './contexts/ScrollContext'
import { Header } from './components/layout/Header'
import { Sidebar } from './components/layout/Sidebar'
import { TasksPage } from './pages/tasks/TasksPage'
import { ScansPage } from './pages/scans/ScansPage'
import { MonitorPage } from './pages/monitor/MonitorPage'
import { ExplorePage } from './pages/explore/ExplorePage'
import { AlertsPage } from './pages/alerts/AlertsPage'
import { InsightsPage } from './pages/insights/InsightsPage'
import { BrowsePage } from './pages/browse/BrowsePage'
import { SettingsPage } from './pages/settings/SettingsPage'
import { BackendUnavailablePage } from './components/layout/BackendUnavailablePage'

function AppContent() {
  const { backendConnected } = useTaskContext()
  const [mainElement, setMainElement] = useState<HTMLElement | null>(null)
  const mainRef = useCallback((node: HTMLElement | null) => {
    setMainElement(node)
  }, [])

  if (!backendConnected) {
    return <BackendUnavailablePage />
  }

  return (
    <div className="flex h-screen flex-col bg-background">
      <Header />
      <div className="flex flex-1 overflow-hidden">
        <Sidebar />
        <main ref={mainRef} className="flex-1 overflow-auto bg-background p-6">
          <ScrollContext.Provider value={mainElement}>
          <Routes>
            <Route path="/" element={<TasksPage />} />
            <Route path="/scans" element={<ScansPage />} />
            <Route path="/monitor" element={<MonitorPage />} />
            <Route path="/explore/*" element={<ExplorePage />} />
            <Route path="/alerts" element={<AlertsPage />} />
            <Route path="/insights/*" element={<InsightsPage />} />
            <Route path="/browse" element={<BrowsePage />} />
            <Route path="/settings" element={<SettingsPage />} />
          </Routes>
          </ScrollContext.Provider>
        </main>
      </div>
    </div>
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
