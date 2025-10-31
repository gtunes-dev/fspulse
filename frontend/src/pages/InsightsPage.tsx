import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { ScanTrendsTab } from './insights/ScanTrendsTab'

const VALID_TABS = ['scan-trends', 'statistics', 'changes'] as const
type TabValue = typeof VALID_TABS[number]

export function InsightsPage() {
  const location = useLocation()
  const navigate = useNavigate()

  // Extract current tab from URL path
  const pathParts = location.pathname.split('/').filter(Boolean)
  const currentTab = pathParts[1] as TabValue | undefined

  // Redirect to default tab if none specified or invalid
  useEffect(() => {
    if (!currentTab || !VALID_TABS.includes(currentTab)) {
      navigate('/insights/scan-trends', { replace: true })
    }
  }, [currentTab, navigate])

  // Handle tab changes by updating URL
  const handleTabChange = (value: string) => {
    navigate(`/insights/${value}`)
  }

  // If no valid tab yet, don't render tabs (will redirect)
  if (!currentTab || !VALID_TABS.includes(currentTab)) {
    return null
  }

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Insights</h1>

      <Tabs value={currentTab} onValueChange={handleTabChange} className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="scan-trends">Scan Trends</TabsTrigger>
          <TabsTrigger value="statistics">Statistics</TabsTrigger>
          <TabsTrigger value="changes">Changes</TabsTrigger>
        </TabsList>

        <TabsContent value="scan-trends" className="flex-1">
          <ScanTrendsTab />
        </TabsContent>

        <TabsContent value="statistics" className="flex-1">
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Coming soon
          </div>
        </TabsContent>

        <TabsContent value="changes" className="flex-1">
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Coming soon
          </div>
        </TabsContent>
      </Tabs>
    </div>
  )
}
