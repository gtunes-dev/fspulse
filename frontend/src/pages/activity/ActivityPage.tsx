import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { CirclePause } from 'lucide-react'
import { ScanCard } from './ScanCard'
import { ScanHistoryTable } from './ScanHistoryTable'
import { UpcomingScansTable } from './UpcomingScansTable'
import { EmptyStateNoRoots } from './EmptyStateNoRoots'
import { EmptyStateNoScans } from './EmptyStateNoScans'
import { PauseScanDialog } from './PauseScanDialog'
import { Card, CardContent } from '@/components/ui/card'
import { InfoBar } from '@/components/shared/InfoBar'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { countQuery } from '@/lib/api'

/**
 * Activity Page - Main landing page showing scan activity and status
 *
 * DEVELOPMENT/TESTING URL PARAMETERS:
 * ------------------------------------
 * You can override the displayed state using URL parameters for testing/development:
 *
 * - ?state=no-roots    Force display of "No Roots" empty state
 * - ?state=no-scans    Force display of "No Scans" empty state
 *
 * Examples:
 *   http://localhost:5173/?state=no-roots
 *   http://localhost:5173/?state=no-scans
 *
 * Without any parameter, the page uses actual database counts to determine state.
 */
export function ActivityPage() {
  const [searchParams] = useSearchParams()
  const { isPaused, pauseUntil } = useScanManager()
  const [loading, setLoading] = useState(true)
  const [rootCount, setRootCount] = useState(0)
  const [scanCount, setScanCount] = useState(0)
  const [showPauseDialog, setShowPauseDialog] = useState(false)

  // Check for state override parameter (for testing/development)
  const stateOverride = searchParams.get('state')

  // Format pause duration for banner
  const getPauseDuration = () => {
    if (!isPaused) return ''
    if (pauseUntil === -1) return 'indefinitely'
    if (pauseUntil !== null) {
      return `until ${new Date(pauseUntil * 1000).toLocaleString()}`
    }
    return ''
  }

  useEffect(() => {
    async function loadCounts() {
      try {
        setLoading(true)

        // Count roots
        const rootsCount = await countQuery('roots', {
          columns: [{ name: 'root_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [],
        })
        setRootCount(rootsCount.count)

        // Count all scans (including in-progress)
        const scansCount = await countQuery('scans', {
          columns: [{ name: 'scan_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [],
        })
        setScanCount(scansCount.count)
      } catch (err) {
        console.error('Error loading counts:', err)
      } finally {
        setLoading(false)
      }
    }

    loadCounts()
  }, [])

  // Show loading state (unless overridden for testing)
  if (loading && !stateOverride) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <Card>
          <CardContent className="pt-6">
            <p className="text-sm text-muted-foreground text-center py-8">
              Loading...
            </p>
          </CardContent>
        </Card>
      </div>
    )
  }

  // Apply state override if present (for testing/development)
  // This allows viewing different empty states without modifying the database
  if (stateOverride === 'no-roots') {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <EmptyStateNoRoots />
      </div>
    )
  }

  if (stateOverride === 'no-scans') {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <ScanCard />
        <EmptyStateNoScans rootCount={rootCount || 2} />
      </div>
    )
  }

  // Normal logic based on actual counts
  // Empty state: No roots configured and no scans in history
  if (rootCount === 0 && scanCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <EmptyStateNoRoots />
      </div>
    )
  }

  // Empty state: Has roots but no scans
  // Show the Scans card so they can start a manual scan
  if (scanCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <ScanCard />
        <EmptyStateNoScans rootCount={rootCount} />
      </div>
    )
  }

  // Normal operational state
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Activity</h1>

      {/* Global Pause Banner - Only when paused */}
      {isPaused && (
        <InfoBar variant="warning" icon={CirclePause}>
          <span className="font-semibold">System Paused</span>
          {' — '}
          Scanning paused {getPauseDuration()}
          {' • '}
          <button
            onClick={() => setShowPauseDialog(true)}
            className="underline hover:text-purple-800 dark:hover:text-purple-200"
          >
            Edit Pause
          </button>
        </InfoBar>
      )}

      {/* Unified Scan Card - Always visible, handles both active and idle states */}
      <ScanCard />

      {/* Upcoming Scans */}
      <UpcomingScansTable />

      {/* Scan History */}
      <ScanHistoryTable />

      {/* Pause Dialog */}
      <PauseScanDialog open={showPauseDialog} onOpenChange={setShowPauseDialog} />
    </div>
  )
}
