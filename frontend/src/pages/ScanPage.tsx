import { useState } from 'react'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { ActiveScanCard } from '@/components/scan/ActiveScanCard'
import { RootsTable } from '@/components/scan/RootsTable'
import { AddRootDialog } from '@/components/scan/AddRootDialog'
import { ScanOptionsDialog } from '@/components/scan/ScanOptionsDialog'
import type { ScheduleScanRequest } from '@/lib/types'

export function ScanPage() {
  const { isScanning, checkForActiveScan } = useScanManager()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const [scanOptionsDialogOpen, setScanOptionsDialogOpen] = useState(false)
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null)
  const [scanError, setScanError] = useState<string | null>(null)
  const [isSchedulingScan, setIsSchedulingScan] = useState(false)

  const handleScanClick = (rootId: number, isIncomplete: boolean) => {
    setSelectedRootId(rootId)
    setScanError(null)

    if (isIncomplete) {
      // Resume scan - use default options (backend will continue with existing options)
      scheduleScan(rootId, 'All', 'New or Changed')
    } else {
      // New scan - show options dialog
      setScanOptionsDialogOpen(true)
    }
  }

  const scheduleScan = async (rootId: number, hashMode: string, validateMode: string) => {
    setIsSchedulingScan(true)
    setScanError(null)

    try {
      // Map UI labels to API values
      const hashModeMap: Record<string, string> = {
        'All': 'All',
        'New or Changed': 'New',
        'None': 'None'
      }
      const validateModeMap: Record<string, string> = {
        'All': 'All',
        'New or Changed': 'New',
        'None': 'None'
      }

      const request: ScheduleScanRequest = {
        root_id: rootId,
        hash_mode: hashModeMap[hashMode] as 'All' | 'New' | 'None',
        validate_mode: validateModeMap[validateMode] as 'All' | 'New' | 'None'
      }

      const response = await fetch('/api/scans/schedule', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request)
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`Failed to schedule scan: ${response.statusText} - ${errorText}`)
      }

      // Scan was scheduled successfully
      // Check if it started (should happen immediately for manual scans)
      await checkForActiveScan()
    } catch (error) {
      console.error('Error scheduling scan:', error)
      setScanError(error instanceof Error ? error.message : 'Failed to schedule scan')
    } finally {
      setIsSchedulingScan(false)
    }
  }

  const handleScheduleScan = (hashMode: string, validateMode: string) => {
    if (selectedRootId !== null) {
      scheduleScan(selectedRootId, hashMode, validateMode)
    }
    setScanOptionsDialogOpen(false)
    setSelectedRootId(null)
  }

  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Scan</h1>

      {/* Error Message */}
      {scanError && (
        <div className="bg-red-50 dark:bg-red-950/30 border border-red-200 dark:border-red-800 rounded-md p-4">
          <div className="text-sm text-red-600 dark:text-red-400 font-medium mb-1">
            Failed to Schedule Scan
          </div>
          <div className="text-sm text-red-600 dark:text-red-400">
            {scanError}
          </div>
        </div>
      )}

      {/* Active Scan Card */}
      <ActiveScanCard />

      {/* Roots Table */}
      <RootsTable
        onAddRoot={() => setAddRootDialogOpen(true)}
        onScanClick={handleScanClick}
        isScanning={isScanning || isSchedulingScan}
      />

      {/* Add Root Dialog */}
      <AddRootDialog
        open={addRootDialogOpen}
        onOpenChange={setAddRootDialogOpen}
      />

      {/* Scan Options Dialog */}
      <ScanOptionsDialog
        open={scanOptionsDialogOpen}
        onOpenChange={setScanOptionsDialogOpen}
        onConfirm={handleScheduleScan}
      />
    </div>
  )
}
