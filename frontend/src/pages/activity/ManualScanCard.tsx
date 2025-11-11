import { useState } from 'react'
import { Link } from 'react-router-dom'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { ManualScanDialog } from './ManualScanDialog'
import { Play, Radar } from 'lucide-react'
import { useScanManager } from '@/contexts/ScanManagerContext'

/**
 * ScansCard - Actions card for scan management
 *
 * Contains two side-by-side cards:
 * 1. Manual Scan - Start one-time scans
 * 2. Recurring Schedules - Link to Monitor page for schedule management
 */
export function ScansCard() {
  const { isScanning } = useScanManager()
  const [scanDialogOpen, setScanDialogOpen] = useState(false)

  const manualScanSubtitle = isScanning
    ? 'Will start after current scan completes'
    : 'Start a one-time scan now'

  return (
    <>
      <Card>
        <CardHeader>
          <CardTitle>Scans</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Manual Scan Card */}
            <div className="border border-border rounded-lg p-4 flex flex-col items-center justify-center gap-3 min-h-[160px]">
              <Button
                size="lg"
                onClick={() => setScanDialogOpen(true)}
              >
                <Play className="h-4 w-4 mr-2" />
                Manual Scan
              </Button>
              <p className="text-sm text-muted-foreground text-center">
                {manualScanSubtitle}
              </p>
            </div>

            {/* Recurring Schedules Card */}
            <Link
              to="/monitor"
              className="border border-border rounded-lg p-4 flex flex-col items-center justify-center gap-3 min-h-[160px] hover:bg-muted/50 transition-colors"
            >
              <div className="flex items-center gap-2 text-primary">
                <Radar className="h-5 w-5" />
                <span className="font-medium">Monitor</span>
              </div>
              <p className="text-sm text-muted-foreground text-center">
                Configure recurring scans on the Monitor page
              </p>
            </Link>
          </div>
        </CardContent>
      </Card>

      {/* Manual Scan Dialog */}
      <ManualScanDialog open={scanDialogOpen} onOpenChange={setScanDialogOpen} />
    </>
  )
}
