import { RootsTable } from '@/components/scan/RootsTable'
import { SchedulesTable } from '@/components/scan/SchedulesTable'
import { AddRootDialog } from '@/components/scan/AddRootDialog'
import { useState, useRef } from 'react'
import { useScanManager } from '@/contexts/ScanManagerContext'

export function MonitorPage() {
  const { isScanning } = useScanManager()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const schedulesTableRef = useRef<{ reload: () => void }>(null)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Monitor</h1>

      <RootsTable
        onAddRoot={() => setAddRootDialogOpen(true)}
      />

      <SchedulesTable
        isScanning={isScanning}
        ref={schedulesTableRef}
      />

      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
      />
    </div>
  )
}
