import { BrowserRouter, Routes, Route } from 'react-router-dom'
import { ScanManagerProvider } from './contexts/ScanManagerContext'
import { Header } from './components/Header'
import { Sidebar } from './components/Sidebar'
import { HomePage } from './pages/HomePage'
import { ExplorePage } from './pages/ExplorePage'
import { ScanPage } from './pages/ScanPage'

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
                <Route path="/" element={<HomePage />} />
                <Route path="/explore" element={<ExplorePage />} />
                <Route path="/scan" element={<ScanPage />} />
                <Route path="/insights" element={<div>Insights page coming soon...</div>} />
                <Route path="/settings" element={<div>Settings page coming soon...</div>} />
              </Routes>
            </main>
          </div>
        </div>
      </ScanManagerProvider>
    </BrowserRouter>
  )
}

export default App
