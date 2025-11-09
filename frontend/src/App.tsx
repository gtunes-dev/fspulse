import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { ScanManagerProvider } from './contexts/ScanManagerContext'
import { Header } from './components/Header'
import { Sidebar } from './components/Sidebar'
import { ActivityPage } from './pages/ActivityPage'
import { MonitorPage } from './pages/MonitorPage'
import { ExplorePage } from './pages/ExplorePage'
import { AlertsPage } from './pages/AlertsPage'
import { InsightsPage } from './pages/InsightsPage'
import { BrowsePage } from './pages/BrowsePage'
import { SettingsPage } from './pages/SettingsPage'

function App() {
  return (
    <BrowserRouter>
      <ScanManagerProvider>
        <div className="flex h-screen flex-col bg-background">
          <Header />
          <div className="flex flex-1 overflow-hidden">
            <Sidebar />
            <main className="flex-1 overflow-auto bg-background p-6">
              <Routes>
                <Route path="/" element={<ActivityPage />} />
                <Route path="/monitor" element={<MonitorPage />} />
                <Route path="/explore/*" element={<ExplorePage />} />
                <Route path="/alerts" element={<AlertsPage />} />
                <Route path="/insights/*" element={<InsightsPage />} />
                <Route path="/browse" element={<BrowsePage />} />
                <Route path="/settings" element={<SettingsPage />} />
              </Routes>
            </main>
          </div>
        </div>
      </ScanManagerProvider>
    </BrowserRouter>
  )
}

export default App
