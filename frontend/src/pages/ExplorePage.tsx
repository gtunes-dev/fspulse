import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { RootsView } from './explore/RootsView'
import { ScansView } from './explore/ScansView'
import { ItemsView } from './explore/ItemsView'
import { ChangesView } from './explore/ChangesView'
import { AlertsView } from './explore/AlertsView'

export function ExplorePage() {
  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Explore</h1>

      <Tabs defaultValue="roots" className="flex-1 flex flex-col">
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

        <TabsContent value="query">
          <Card>
            <CardHeader>
              <CardTitle>Query</CardTitle>
            </CardHeader>
            <CardContent>
              <p className="text-muted-foreground">Query interface will appear here...</p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>
    </div>
  )
}
