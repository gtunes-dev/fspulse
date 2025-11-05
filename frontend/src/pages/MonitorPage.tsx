import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { RootsTable } from '@/components/scan/RootsTable'
import { AddRootDialog } from '@/components/scan/AddRootDialog'
import { useState } from 'react'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { Card, CardContent } from '@/components/ui/card'

export function MonitorPage() {
  const { isScanning } = useScanManager()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Monitor</h1>

      <Tabs defaultValue="roots" className="space-y-4">
        <TabsList>
          <TabsTrigger value="roots">Roots</TabsTrigger>
          <TabsTrigger value="schedules">Schedules</TabsTrigger>
        </TabsList>

        <TabsContent value="roots" className="space-y-4">
          <RootsTable
            onAddRoot={() => setAddRootDialogOpen(true)}
            isScanning={isScanning}
          />
        </TabsContent>

        <TabsContent value="schedules" className="space-y-4">
          <Card>
            <CardContent className="pt-6">
              <p className="text-sm text-muted-foreground text-center py-8">
                Schedule management will appear here
              </p>
            </CardContent>
          </Card>
        </TabsContent>
      </Tabs>

      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
      />
    </div>
  )
}
