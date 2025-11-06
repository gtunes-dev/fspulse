import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { ActiveScanCard } from '@/components/scan/ActiveScanCard'
import { ScansCard } from '@/components/scan/ManualScanCard'
import { RecentScansTable } from '@/components/activity/RecentScansTable'
import { UpcomingScansTable } from '@/components/activity/UpcomingScansTable'
import { EmptyStateNoRoots } from '@/components/activity/EmptyStateNoRoots'
import { EmptyStateNoScans } from '@/components/activity/EmptyStateNoScans'
import { Card, CardContent } from '@/components/ui/card'
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
  const [loading, setLoading] = useState(true)
  const [rootCount, setRootCount] = useState(0)
  const [scanCount, setScanCount] = useState(0)

  // Check for state override parameter (for testing/development)
  const stateOverride = searchParams.get('state')

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

        // Count completed scans
        const scansCount = await countQuery('scans', {
          columns: [{ name: 'scan_id', visible: true, sort_direction: 'none', position: 0 }],
          filters: [{ column: 'scan_state', value: 'C,P,E' }],
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
        <EmptyStateNoScans rootCount={rootCount || 2} />
      </div>
    )
  }

  // Normal logic based on actual counts
  // Empty state: No roots configured
  if (rootCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <EmptyStateNoRoots />
      </div>
    )
  }

  // Empty state: Has roots but no scans
  if (scanCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Activity</h1>
        <EmptyStateNoScans rootCount={rootCount} />
      </div>
    )
  }

  // Normal operational state
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Activity</h1>

      {/* Scans Action Card - Always visible */}
      <ScansCard />

      {/* Active Scan Status Card - Always visible, handles both states internally */}
      <ActiveScanCard />

      {/* Upcoming Scans */}
      <UpcomingScansTable />

      {/* Recent Scans */}
      <RecentScansTable />
    </div>
  )
}
