import { useState } from 'react'
import { RootsTable } from './RootsTable'
import { AddRootDialog } from './AddRootDialog'
import { ManualScanDialog } from '../dashboard/ManualScanDialog'

export function RootsPage() {
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const [rootsReloadTrigger, setRootsReloadTrigger] = useState(0)
  const [scanDialogOpen, setScanDialogOpen] = useState(false)
  const [scanRootId, setScanRootId] = useState<number | undefined>(undefined)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Roots</h1>

      <RootsTable
        onAddRoot={() => setAddRootDialogOpen(true)}
        onScanNow={(rootId?: number) => {
          setScanRootId(rootId)
          setScanDialogOpen(true)
        }}
        externalReloadTrigger={rootsReloadTrigger}
      />

      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
        onSuccess={() => setRootsReloadTrigger(prev => prev + 1)}
      />

      <ManualScanDialog
        open={scanDialogOpen}
        onOpenChange={setScanDialogOpen}
        preselectedRootId={scanRootId}
      />
    </div>
  )
}
