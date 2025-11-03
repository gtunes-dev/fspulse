import { useState } from 'react'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { ActiveScanCard } from '@/components/scan/ActiveScanCard'
import { RootsTable } from '@/components/scan/RootsTable'
import { AddRootDialog } from '@/components/scan/AddRootDialog'
import { ScanOptionsDialog } from '@/components/scan/ScanOptionsDialog'
import type { InitiateScanRequest, RootWithScan } from '@/lib/types'

export function ScanPage() {
  const { isScanning, connectScanWebSocket } = useScanManager()
  const [addRootDialogOpen, setAddRootDialogOpen] = useState(false)
  const [scanOptionsDialogOpen, setScanOptionsDialogOpen] = useState(false)
  const [selectedRootId, setSelectedRootId] = useState<number | null>(null)
  const [scanError, setScanError] = useState<string | null>(null)
  const [isStartingScan, setIsStartingScan] = useState(false)

  const handleScanClick = (rootId: number, isIncomplete: boolean) => {
    setSelectedRootId(rootId)
    setScanError(null)

    if (isIncomplete) {
      // Resume scan - use default options (backend will continue with existing options)
      startScan(rootId, 'All', 'New or Changed')
    } else {
      // New scan - show options dialog
      setScanOptionsDialogOpen(true)
    }
  }

  const startScan = async (rootId: number, hashMode: string, validateMode: string) => {
    setIsStartingScan(true)
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

      const request: InitiateScanRequest = {
        root_id: rootId,
        hash_mode: hashModeMap[hashMode] as 'All' | 'New' | 'None',
        validate_mode: validateModeMap[validateMode] as 'All' | 'New' | 'None'
      }

      const response = await fetch('/api/scans/start', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(request)
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`Failed to start scan: ${response.statusText} - ${errorText}`)
      }

      const data = await response.json()

      // Get root path for WebSocket connection
      // We'll need to fetch it or pass it through
      const rootResponse = await fetch('/api/roots/with-scans')
      const roots: RootWithScan[] = await rootResponse.json()
      const root = roots.find((r) => r.root_id === rootId)

      if (root) {
        // Connect WebSocket to start receiving updates
        connectScanWebSocket(data.scan_id, root.root_path)
      }
    } catch (error) {
      console.error('Error starting scan:', error)
      setScanError(error instanceof Error ? error.message : 'Failed to start scan')
    } finally {
      setIsStartingScan(false)
    }
  }

  const handleStartScan = (hashMode: string, validateMode: string) => {
    if (selectedRootId !== null) {
      startScan(selectedRootId, hashMode, validateMode)
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
            Failed to Start Scan
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
        isScanning={isScanning || isStartingScan}
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
        onConfirm={handleStartScan}
      />
    </div>
  )
}
