import { useState, useEffect } from 'react'
import { useSearchParams } from 'react-router-dom'
import { CirclePause } from 'lucide-react'
import { TaskCard } from './TaskCard'
import { TaskHistoryTable } from './TaskHistoryTable'
import { UpcomingTasksTable } from './UpcomingTasksTable'
import { EmptyStateNoRoots } from './EmptyStateNoRoots'
import { EmptyStateNoTasks } from './EmptyStateNoTasks'
import { PauseDialog } from './PauseDialog'
import { Card, CardContent } from '@/components/ui/card'
import { InfoBar } from '@/components/shared/InfoBar'
import { useTaskContext } from '@/contexts/TaskContext'
import { countQuery } from '@/lib/api'

/**
 * Tasks Page - Main landing page showing task status and history
 *
 * DEVELOPMENT/TESTING URL PARAMETERS:
 * ------------------------------------
 * You can override the displayed state using URL parameters for testing/development:
 *
 * - ?state=no-roots    Force display of "No Roots" empty state
 * - ?state=no-tasks    Force display of "No Tasks" empty state
 *
 * Examples:
 *   http://localhost:5173/?state=no-roots
 *   http://localhost:5173/?state=no-tasks
 *
 * Without any parameter, the page uses actual database counts to determine state.
 */
export function TasksPage() {
  const [searchParams] = useSearchParams()
  const { isPaused, pauseUntil } = useTaskContext()
  const [loading, setLoading] = useState(true)
  const [rootCount, setRootCount] = useState(0)
  const [taskCount, setTaskCount] = useState(0)
  const [showPauseDialog, setShowPauseDialog] = useState(false)

  // Check for state override parameter (for testing/development)
  const stateOverride = searchParams.get('state')

  // Format pause duration for banner
  const getPauseDuration = () => {
    if (!isPaused) return ''
    if (pauseUntil === -1) return 'indefinitely'
    if (pauseUntil !== null) {
      // Calculate friendly duration
      const now = Date.now() / 1000  // Convert to seconds
      const diff = pauseUntil - now

      const minutes = Math.floor(diff / 60)
      const hours = Math.floor(diff / 3600)
      const days = Math.floor(diff / 86400)

      let duration = ''
      if (minutes < 60) {
        duration = `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`
      } else if (hours < 24) {
        duration = `${hours} ${hours === 1 ? 'hour' : 'hours'}`
      } else {
        duration = `${days} ${days === 1 ? 'day' : 'days'}`
      }

      const untilDate = new Date(pauseUntil * 1000).toLocaleString()
      return `for ${duration} (until ${untilDate})`
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

        // Count completed tasks via the task history endpoint
        const countResponse = await fetch('/api/tasks/history/count')
        if (countResponse.ok) {
          const countData = await countResponse.json()
          setTaskCount(countData.count)
        }
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
        <h1 className="text-2xl font-semibold">Tasks</h1>
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
  if (stateOverride === 'no-roots') {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Tasks</h1>
        <EmptyStateNoRoots />
      </div>
    )
  }

  if (stateOverride === 'no-tasks') {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Tasks</h1>
        <TaskCard />
        <EmptyStateNoTasks rootCount={rootCount || 2} />
      </div>
    )
  }

  // Normal logic based on actual counts
  // Empty state: No roots configured and no tasks in history
  if (rootCount === 0 && taskCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Tasks</h1>
        <EmptyStateNoRoots />
      </div>
    )
  }

  // Empty state: Has roots but no tasks
  // Show the TaskCard so they can start a manual scan
  if (taskCount === 0) {
    return (
      <div className="flex flex-col gap-6">
        <h1 className="text-2xl font-semibold">Tasks</h1>
        <TaskCard />
        <EmptyStateNoTasks rootCount={rootCount} />
      </div>
    )
  }

  // Normal operational state
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Tasks</h1>

      {/* Global Pause Banner - Only when paused */}
      {isPaused && (
        <InfoBar variant="warning" icon={CirclePause}>
          <span className="font-semibold">Tasks Paused</span>
          {' — '}
          Tasks paused {getPauseDuration()}
          {' • '}
          <button
            onClick={() => setShowPauseDialog(true)}
            className="underline hover:text-purple-800 dark:hover:text-purple-200"
          >
            Edit Pause
          </button>
        </InfoBar>
      )}

      {/* Task Card - Always visible, handles both active and idle states */}
      <TaskCard />

      {/* Upcoming Tasks */}
      <UpcomingTasksTable />

      {/* Task History */}
      <TaskHistoryTable />

      {/* Pause Dialog */}
      <PauseDialog open={showPauseDialog} onOpenChange={setShowPauseDialog} />
    </div>
  )
}
