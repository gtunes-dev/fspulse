import { useState, useEffect, useRef, forwardRef, useImperativeHandle, useMemo } from 'react'
import { Trash2, Power, Pencil } from 'lucide-react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { DeleteScheduleDialog } from './DeleteScheduleDialog'
import { EditScheduleDialog } from './EditScheduleDialog'
import { formatDateRelative } from '@/lib/dateUtils'
import type { ScheduleWithRoot } from '@/lib/types'

interface SchedulesTableProps {
  isScanning: boolean
}

export interface SchedulesTableRef {
  reload: () => void
}

const ITEMS_PER_PAGE = 25

export const SchedulesTable = forwardRef<SchedulesTableRef, SchedulesTableProps>(
  function SchedulesTable({ isScanning }, ref) {
    const [schedules, setSchedules] = useState<ScheduleWithRoot[]>([])
    const [loading, setLoading] = useState(true)
    const [currentPage, setCurrentPage] = useState(1)
    const [selectedRootId, setSelectedRootId] = useState<string>('all')
    const [deleteDialogOpen, setDeleteDialogOpen] = useState(false)
    const [editDialogOpen, setEditDialogOpen] = useState(false)
    const [selectedSchedule, setSelectedSchedule] = useState<ScheduleWithRoot | null>(null)

    const loadSchedules = async () => {
      try {
        setLoading(true)
        const response = await fetch('/api/schedules')
        if (!response.ok) throw new Error('Failed to load schedules')

        const data: ScheduleWithRoot[] = await response.json()
        setSchedules(data)
      } catch (error) {
        console.error('Error loading schedules:', error)
      } finally {
        setLoading(false)
      }
    }

    // Extract unique roots for the filter dropdown
    const uniqueRoots = useMemo(() => {
      const rootMap = new Map<number, string>()
      schedules.forEach(schedule => {
        if (!rootMap.has(schedule.root_id)) {
          rootMap.set(schedule.root_id, schedule.root_path)
        }
      })
      return Array.from(rootMap.entries()).map(([id, path]) => ({ id, path }))
    }, [schedules])

    // Filter schedules by selected root
    const filteredSchedules = useMemo(() => {
      if (selectedRootId === 'all') {
        return schedules
      }
      return schedules.filter(s => s.root_id === parseInt(selectedRootId))
    }, [schedules, selectedRootId])

    // Expose reload method via ref
    useImperativeHandle(ref, () => ({
      reload: loadSchedules
    }))

    useEffect(() => {
      loadSchedules()
    }, [])

    // Track previous scan state using a ref to detect completion
    const wasScanningRef = useRef(isScanning)

    useEffect(() => {
      // Detect scan completion (was scanning, now not scanning)
      if (wasScanningRef.current && !isScanning) {
        console.log('Scan completed, reloading schedules data')
        // Give backend time to finish writing to database
        const timer = setTimeout(() => {
          loadSchedules()
        }, 1500)

        wasScanningRef.current = isScanning
        return () => clearTimeout(timer)
      }

      // Update ref for next check
      wasScanningRef.current = isScanning
    }, [isScanning])

    // Reset to page 1 when filter changes
    useEffect(() => {
      setCurrentPage(1)
    }, [selectedRootId])

    // Pagination (using filtered schedules)
    const totalCount = filteredSchedules.length
    const startIndex = (currentPage - 1) * ITEMS_PER_PAGE
    const endIndex = Math.min(startIndex + ITEMS_PER_PAGE, totalCount)
    const paginatedSchedules = filteredSchedules.slice(startIndex, endIndex)

    // Format schedule description
    const formatScheduleDescription = (schedule: ScheduleWithRoot): string => {
      switch (schedule.schedule_type) {
        case 'Daily':
          return `Daily at ${schedule.time_of_day}`
        case 'Weekly':
          try {
            const days = schedule.days_of_week ? JSON.parse(schedule.days_of_week) : []
            return `Weekly on ${days.join(', ')} at ${schedule.time_of_day}`
          } catch {
            return 'Weekly'
          }
        case 'Monthly':
          return `Monthly on day ${schedule.day_of_month} at ${schedule.time_of_day}`
        case 'Interval':
          return `Every ${schedule.interval_value} ${schedule.interval_unit?.toLowerCase()}`
        default:
          return schedule.schedule_type
      }
    }

    // Toggle enabled handler
    const handleToggleEnabled = async (schedule: ScheduleWithRoot) => {
      try {
        const response = await fetch(`/api/schedules/${schedule.schedule_id}/toggle`, {
          method: 'PATCH',
          headers: {
            'Content-Type': 'application/json',
          },
          body: JSON.stringify({ enabled: !schedule.enabled }),
        })

        if (!response.ok) throw new Error('Failed to toggle schedule')

        // Reload schedules
        await loadSchedules()
      } catch (error) {
        console.error('Error toggling schedule:', error)
        alert('Failed to toggle schedule')
      }
    }

    if (loading) {
      return (
        <Card>
          <CardContent className="py-8">
            <div className="text-center text-muted-foreground">Loading schedules...</div>
          </CardContent>
        </Card>
      )
    }

    return (
      <>
      <Card>
        <CardHeader>
          <CardTitle>Schedules</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Root Filter */}
          {uniqueRoots.length > 0 && (
            <div className="flex items-center gap-2">
              <span className="text-sm text-muted-foreground">Filter by root:</span>
              <Select value={selectedRootId} onValueChange={setSelectedRootId}>
                <SelectTrigger className="w-[300px]">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="all">All Roots</SelectItem>
                  {uniqueRoots.map(root => (
                    <SelectItem key={root.id} value={root.id.toString()}>
                      {root.path}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {paginatedSchedules.length === 0 ? (
            <div className="border border-border rounded-lg">
              <p className="text-sm text-muted-foreground text-center py-12">
                {selectedRootId === 'all'
                  ? 'No schedules found. Click "Add Schedule" to get started.'
                  : 'No schedules found for this root.'}
              </p>
            </div>
          ) : (
            <>
              {/* Bordered Table Container */}
              <div className="border border-border rounded-lg overflow-hidden">
                <Table>
                  <TableHeader className="bg-muted">
                    <TableRow>
                      <TableHead className="w-10"></TableHead>
                      <TableHead className="w-10"></TableHead>
                      <TableHead className="w-10"></TableHead>
                      <TableHead className="uppercase text-xs tracking-wide">Name</TableHead>
                      <TableHead className="uppercase text-xs tracking-wide">Root</TableHead>
                      <TableHead className="uppercase text-xs tracking-wide">Schedule</TableHead>
                      <TableHead className="uppercase text-xs tracking-wide">Next Scan</TableHead>
                      <TableHead className="uppercase text-xs tracking-wide text-center">Status</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {paginatedSchedules.map((schedule) => (
                      <TableRow key={schedule.schedule_id}>
                        {/* Toggle Enabled Column */}
                        <TableCell className="w-10 pr-2">
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={isScanning}
                            onClick={() => handleToggleEnabled(schedule)}
                            className={`h-8 w-8 p-0 ${
                              schedule.enabled
                                ? 'text-green-600 hover:text-green-700 hover:bg-green-100'
                                : 'text-muted-foreground hover:text-foreground hover:bg-muted'
                            }`}
                            title={schedule.enabled ? 'Disable schedule' : 'Enable schedule'}
                          >
                            <Power className="h-5 w-5" />
                          </Button>
                        </TableCell>

                        {/* Delete Icon Column */}
                        <TableCell className="w-10 pr-2">
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={isScanning}
                            onClick={() => {
                              setSelectedSchedule(schedule)
                              setDeleteDialogOpen(true)
                            }}
                            className="h-8 w-8 p-0 text-muted-foreground hover:text-destructive hover:bg-destructive/10"
                            title="Delete schedule"
                          >
                            <Trash2 className="h-5 w-5" />
                          </Button>
                        </TableCell>

                        {/* Edit Icon Column */}
                        <TableCell className="w-10 pr-2">
                          <Button
                            size="sm"
                            variant="ghost"
                            disabled={isScanning}
                            onClick={() => {
                              setSelectedSchedule(schedule)
                              setEditDialogOpen(true)
                            }}
                            className="h-8 w-8 p-0 text-muted-foreground hover:text-primary hover:bg-primary/10"
                            title="Edit schedule"
                          >
                            <Pencil className="h-5 w-5" />
                          </Button>
                        </TableCell>

                        {/* Name Column */}
                        <TableCell>
                          <span className="font-medium">
                            {schedule.schedule_name}
                          </span>
                        </TableCell>

                        {/* Root Path Column */}
                        <TableCell>
                          <span className="text-sm text-muted-foreground">
                            {schedule.root_path}
                          </span>
                        </TableCell>

                        {/* Schedule Description Column */}
                        <TableCell>
                          <span className="text-sm">
                            {formatScheduleDescription(schedule)}
                          </span>
                        </TableCell>

                        {/* Next Scan Column */}
                        <TableCell>
                          {schedule.next_scan_time ? (
                            <span className="text-sm">
                              {formatDateRelative(schedule.next_scan_time)}
                            </span>
                          ) : (
                            <span className="text-sm text-muted-foreground">Not scheduled</span>
                          )}
                        </TableCell>

                        {/* Status Column */}
                        <TableCell className="text-center">
                          {schedule.enabled ? (
                            <Badge variant="success">Enabled</Badge>
                          ) : (
                            <Badge variant="secondary">Disabled</Badge>
                          )}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </div>

              {/* Pagination */}
              {totalCount > ITEMS_PER_PAGE && (
                <div className="flex items-center justify-between pt-4">
                  <div className="text-sm text-muted-foreground">
                    Showing {startIndex + 1} - {endIndex} of {totalCount} schedules
                  </div>
                  <div className="flex items-center gap-2">
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => currentPage > 1 && setCurrentPage(p => p - 1)}
                      disabled={currentPage === 1}
                    >
                      Previous
                    </Button>
                    <Button
                      size="sm"
                      variant="outline"
                      onClick={() => endIndex < totalCount && setCurrentPage(p => p + 1)}
                      disabled={endIndex >= totalCount}
                    >
                      Next
                    </Button>
                  </div>
                </div>
              )}
            </>
          )}
        </CardContent>
      </Card>

      {/* Delete Schedule Dialog */}
      <DeleteScheduleDialog
        open={deleteDialogOpen}
        onOpenChange={setDeleteDialogOpen}
        scheduleId={selectedSchedule?.schedule_id ?? null}
        scheduleName={selectedSchedule?.schedule_name ?? ''}
        onDeleteSuccess={() => {
          loadSchedules()
          setSelectedSchedule(null)
        }}
      />

      {/* Edit Schedule Dialog */}
      <EditScheduleDialog
        open={editDialogOpen}
        onOpenChange={setEditDialogOpen}
        schedule={selectedSchedule}
        onSuccess={() => {
          loadSchedules()
          setSelectedSchedule(null)
        }}
      />
    </>
    )
  }
)

