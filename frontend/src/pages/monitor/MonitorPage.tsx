import { RootsTable } from './RootsTable'
import { SchedulesTable } from './SchedulesTable'
import { AddRootDialog } from './AddRootDialog'
import { useState, useRef } from 'react'
import { useTaskContext } from '@/contexts/TaskContext'

export function MonitorPage() {
  const { isRunning } = useTaskContext()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const schedulesTableRef = useRef<{ reload: () => void }>(null)
  const [rootsReloadTrigger, setRootsReloadTrigger] = useState(0)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Monitor</h1>

      <RootsTable
        onAddRoot={() => setAddRootDialogOpen(true)}
        onScheduleCreated={() => schedulesTableRef.current?.reload()}
        externalReloadTrigger={rootsReloadTrigger}
      />

      <SchedulesTable
        isScanning={isRunning}
        ref={schedulesTableRef}
      />

      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
        onSuccess={() => setRootsReloadTrigger(prev => prev + 1)}
      />
    </div>
  )
}
