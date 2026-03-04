import { useState, useRef } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import { useTaskContext } from '@/contexts/TaskContext'
import { RootsTable } from './RootsTable'
import { SchedulesTable } from './SchedulesTable'
import { AddRootDialog } from './AddRootDialog'
import { SettingsContent } from './SettingsContent'

export function SetupPage() {
  const { isRunning } = useTaskContext()
  const [searchParams, setSearchParams] = useSearchParams()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const schedulesTableRef = useRef<{ reload: () => void }>(null)
  const [rootsReloadTrigger, setRootsReloadTrigger] = useState(0)

  // Support ?tab= parameter for deep linking (e.g., from Dashboard empty states)
  const activeTab = searchParams.get('tab') || 'roots'

  const handleTabChange = (value: string) => {
    setSearchParams({ tab: value }, { replace: true })
  }

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Setup</h1>

      <Tabs value={activeTab} onValueChange={handleTabChange}>
        <TabsList>
          <TabsTrigger value="roots">Roots</TabsTrigger>
          <TabsTrigger value="schedules">Schedules</TabsTrigger>
          <TabsTrigger value="settings">Settings</TabsTrigger>
        </TabsList>

        <TabsContent value="roots">
          <div className="flex flex-col gap-6">
            <RootsTable
              onAddRoot={() => setAddRootDialogOpen(true)}
              onScheduleCreated={() => schedulesTableRef.current?.reload()}
              externalReloadTrigger={rootsReloadTrigger}
            />
          </div>
        </TabsContent>

        <TabsContent value="schedules">
          <div className="flex flex-col gap-6">
            <SchedulesTable
              isScanning={isRunning}
              ref={schedulesTableRef}
            />
          </div>
        </TabsContent>

        <TabsContent value="settings">
          <SettingsContent />
        </TabsContent>
      </Tabs>

      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
        onSuccess={() => setRootsReloadTrigger(prev => prev + 1)}
      />
    </div>
  )
}
