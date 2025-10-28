import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { RootsView } from './explore/RootsView'
import { ScansView } from './explore/ScansView'
import { ItemsView } from './explore/ItemsView'
import { ChangesView } from './explore/ChangesView'
import { AlertsView } from './explore/AlertsView'
import { QueryView } from './explore/QueryView'

const VALID_TABS = ['roots', 'scans', 'items', 'changes', 'alerts', 'query'] as const
type TabValue = typeof VALID_TABS[number]

export function ExplorePage() {
  const location = useLocation()
  const navigate = useNavigate()

  // Extract current tab from URL path
  const pathParts = location.pathname.split('/').filter(Boolean)
  const currentTab = pathParts[1] as TabValue | undefined

  // Redirect to default tab if none specified or invalid
  useEffect(() => {
    if (!currentTab || !VALID_TABS.includes(currentTab)) {
      navigate('/explore/roots', { replace: true })
    }
  }, [currentTab, navigate])

  // Handle tab changes by updating URL
  const handleTabChange = (value: string) => {
    navigate(`/explore/${value}`)
  }

  // If no valid tab yet, don't render tabs (will redirect)
  if (!currentTab || !VALID_TABS.includes(currentTab)) {
    return null
  }

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Explore</h1>

      <Tabs value={currentTab} onValueChange={handleTabChange} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="roots">Roots</TabsTrigger>
          <TabsTrigger value="scans">Scans</TabsTrigger>
          <TabsTrigger value="items">Items</TabsTrigger>
          <TabsTrigger value="changes">Changes</TabsTrigger>
          <TabsTrigger value="alerts">Alerts</TabsTrigger>
          <TabsTrigger value="query">Query</TabsTrigger>
        </TabsList>

        <TabsContent value="roots" className="flex-1">
          <RootsView />
        </TabsContent>

        <TabsContent value="scans" className="flex-1">
          <ScansView />
        </TabsContent>

        <TabsContent value="items" className="flex-1">
          <ItemsView />
        </TabsContent>

        <TabsContent value="changes" className="flex-1">
          <ChangesView />
        </TabsContent>

        <TabsContent value="alerts" className="flex-1">
          <AlertsView />
        </TabsContent>

        <TabsContent value="query" className="flex-1">
          <QueryView />
        </TabsContent>
      </Tabs>
    </div>
  )
}
