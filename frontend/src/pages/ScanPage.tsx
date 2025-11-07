import { useState } from 'react'
import { ActiveScanCard } from '@/components/scan/ActiveScanCard'
import { RootsTable } from '@/components/scan/RootsTable'
import { AddRootDialog } from '@/components/scan/AddRootDialog'

export function ScanPage() {
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Scan</h1>

      {/* Active Scan Card */}
      <ActiveScanCard />

      {/* Roots Table */}
      <RootsTable
        onAddRoot={() => setAddRootDialogOpen(true)}
      />

      {/* Add Root Dialog */}
      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
      />
    </div>
  )
}
