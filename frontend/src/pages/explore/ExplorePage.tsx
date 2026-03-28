import { useEffect } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { RootsView } from './RootsView'
import { ScansView } from './ScansView'
import { ItemsView } from './ItemsView'
import { VersionsView } from './VersionsView'
import { HashesView } from './HashesView'
import { QueryView } from './QueryView'

const VALID_TABS = ['roots', 'scans', 'items', 'versions', 'hashes', 'query'] as const
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
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Data Explorer</h1>

      <Tabs value={currentTab} onValueChange={handleTabChange}>
        <TabsList>
          <TabsTrigger value="roots">Roots</TabsTrigger>
          <TabsTrigger value="scans">Scans</TabsTrigger>
          <TabsTrigger value="items">Items</TabsTrigger>
          <TabsTrigger value="versions">Versions</TabsTrigger>
          <TabsTrigger value="hashes">Hashes</TabsTrigger>
          <TabsTrigger value="query">Query</TabsTrigger>
        </TabsList>

        {/* Keep all tabs mounted for instant switching - use CSS hiding */}
        <div className={`mt-2 ${currentTab === 'roots' ? '' : 'hidden'}`}>
          <RootsView />
        </div>
        <div className={`mt-2 ${currentTab === 'scans' ? '' : 'hidden'}`}>
          <ScansView />
        </div>
        <div className={`mt-2 ${currentTab === 'items' ? '' : 'hidden'}`}>
          <ItemsView />
        </div>
        <div className={`mt-2 ${currentTab === 'versions' ? '' : 'hidden'}`}>
          <VersionsView />
        </div>
        <div className={`mt-2 ${currentTab === 'hashes' ? '' : 'hidden'}`}>
          <HashesView />
        </div>
        <div className={`mt-2 ${currentTab === 'query' ? '' : 'hidden'}`}>
          <QueryView />
        </div>
      </Tabs>
    </div>
  )
}
