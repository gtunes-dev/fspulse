import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
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
    <div className="flex flex-col flex-1 min-h-0">
      <h1 className="text-2xl font-semibold mb-8">Explore</h1>

      <Card className="flex-1 flex flex-col min-h-0">
        <Tabs value={currentTab} onValueChange={handleTabChange} className="flex-1 flex flex-col">
          <CardHeader className="pb-0">
            <TabsList className="h-auto p-0 bg-transparent gap-0">
              <TabsTrigger value="roots" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Roots
              </TabsTrigger>
              <TabsTrigger value="scans" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Scans
              </TabsTrigger>
              <TabsTrigger value="items" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Items
              </TabsTrigger>
              <TabsTrigger value="changes" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Changes
              </TabsTrigger>
              <TabsTrigger value="alerts" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Alerts
              </TabsTrigger>
              <TabsTrigger value="query" className="text-2xl font-semibold px-6 py-3 rounded-none border-b-2 border-transparent data-[state=active]:border-primary data-[state=active]:bg-transparent">
                Query
              </TabsTrigger>
            </TabsList>
          </CardHeader>

          <CardContent className="flex-1 flex flex-col px-6 pb-6 pt-4">
            <TabsContent value="roots" className="flex-1 mt-0">
              <RootsView />
            </TabsContent>

            <TabsContent value="scans" className="flex-1 mt-0">
              <ScansView />
            </TabsContent>

            <TabsContent value="items" className="flex-1 mt-0">
              <ItemsView />
            </TabsContent>

            <TabsContent value="changes" className="flex-1 mt-0">
              <ChangesView />
            </TabsContent>

            <TabsContent value="alerts" className="flex-1 mt-0">
              <AlertsView />
            </TabsContent>

            <TabsContent value="query" className="flex-1 mt-0">
              <QueryView />
            </TabsContent>
          </CardContent>
        </Tabs>
      </Card>
    </div>
  )
}
