import { useState, useEffect } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { RootPicker } from '@/components/ui/RootPicker'
import { ScanOptionsFields } from './ScanOptionsFields'
import { fetchQuery } from '@/lib/api'
import { Loader2 } from 'lucide-react'
import type { ScheduleType, IntervalUnit } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

interface CreateScheduleDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  onSuccess?: () => void
  preselectedRootId?: number  // Allow preselection from Roots table
}

const DAYS_OF_WEEK = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

export function CreateScheduleDialog({
  open,
  onOpenChange,
  onSuccess,
  preselectedRootId
}: CreateScheduleDialogProps) {
  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [scheduleName, setScheduleName] = useState<string>('')
  const [scheduleType, setScheduleType] = useState<ScheduleType>('Daily')
  const [timeOfDay, setTimeOfDay] = useState<string>('09:00')
  const [selectedDays, setSelectedDays] = useState<string[]>(['Mon', 'Wed', 'Fri'])
  const [dayOfMonth, setDayOfMonth] = useState<number>(1)
  const [intervalValue, setIntervalValue] = useState<number>(2)
  const [intervalUnit, setIntervalUnit] = useState<IntervalUnit>('Hours')
  const [hashMode, setHashMode] = useState<string>('New or Changed')
  const [validateMode, setValidateMode] = useState<string>('New or Changed')

  const [loading, setLoading] = useState(true)
  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Load roots and reset form when dialog opens
  useEffect(() => {
    if (open) {
      // Reset form to defaults
      setScheduleName('')
      setScheduleType('Daily')
      setTimeOfDay('09:00')
      setSelectedDays(['Mon', 'Wed', 'Fri'])
      setDayOfMonth(1)
      setIntervalValue(2)
      setIntervalUnit('Hours')
      setHashMode('New or Changed')
      setValidateMode('New or Changed')
      setError(null)

      // Set preselected root if provided
      if (preselectedRootId) {
        setSelectedRootId(preselectedRootId.toString())
      } else {
        setSelectedRootId('')
      }

      loadRoots()
    }
  }, [open, preselectedRootId])

  async function loadRoots() {
    try {
      setLoading(true)
      setError(null)

      const response = await fetchQuery('roots', {
        columns: [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
        ],
        filters: [],
        limit: 1000,
      })

      const rootData: Root[] = response.rows.map((row) => ({
        root_id: parseInt(row[0]),
        root_path: row[1],
      }))

      setRoots(rootData)
    } catch (err) {
      console.error('Error loading roots:', err)
      setError(err instanceof Error ? err.message : 'Failed to load roots')
    } finally {
      setLoading(false)
    }
  }

  function toggleDay(day: string) {
    setSelectedDays(prev =>
      prev.includes(day) ? prev.filter(d => d !== day) : [...prev, day]
    )
  }

  async function handleSubmit() {
    // Validation
    if (!selectedRootId) {
      setError('Please select a root')
      return
    }

    if (!scheduleName.trim()) {
      setError('Please enter a schedule name')
      return
    }

    if (scheduleType === 'Weekly' && selectedDays.length === 0) {
      setError('Please select at least one day of the week')
      return
    }

    if (scheduleType === 'Monthly' && (dayOfMonth < 1 || dayOfMonth > 31)) {
      setError('Day of month must be between 1 and 31')
      return
    }

    if (scheduleType === 'Interval' && intervalValue < 1) {
      setError('Interval value must be at least 1')
      return
    }

    try {
      setSubmitting(true)
      setError(null)

      // Map UI values to API values
      const mapMode = (mode: string): string => {
        if (mode === 'New or Changed') return 'New'
        return mode // 'All' and 'None' map directly
      }

      // Build request body based on schedule type
      const requestBody: Record<string, unknown> = {
        root_id: parseInt(selectedRootId),
        schedule_name: scheduleName.trim(),
        schedule_type: scheduleType,
        hash_mode: mapMode(hashMode),
        validate_mode: mapMode(validateMode),
      }

      // Add schedule-type-specific fields
      if (scheduleType === 'Daily') {
        requestBody.time_of_day = timeOfDay
      } else if (scheduleType === 'Weekly') {
        requestBody.time_of_day = timeOfDay
        requestBody.days_of_week = JSON.stringify(selectedDays)
      } else if (scheduleType === 'Monthly') {
        requestBody.time_of_day = timeOfDay
        requestBody.day_of_month = dayOfMonth
      } else if (scheduleType === 'Interval') {
        requestBody.interval_value = intervalValue
        requestBody.interval_unit = intervalUnit
      }

      // Create the schedule
      const response = await fetch('/api/schedules', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(requestBody),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`Failed to create schedule: ${errorText}`)
      }

      // Success callback
      onSuccess?.()

      // Close dialog (form will reset when reopened)
      onOpenChange(false)
    } catch (err) {
      console.error('Error creating schedule:', err)
      setError(err instanceof Error ? err.message : 'Failed to create schedule')
    } finally {
      setSubmitting(false)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[550px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {preselectedRootId ? 'Add Schedule' : 'Create Schedule'}
          </DialogTitle>
          <DialogDescription>
            {preselectedRootId
              ? 'Configure a recurring scan schedule for this root'
              : 'Configure a recurring scan schedule for a root directory'}
          </DialogDescription>
        </DialogHeader>

        {loading ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : roots.length === 0 ? (
          <div className="py-8 text-center">
            <p className="text-sm text-muted-foreground">
              No roots configured. Please add a root directory first.
            </p>
          </div>
        ) : (
          <div className="space-y-6">
            {/* Schedule Name */}
            <div className="space-y-2">
              <label className="text-sm font-semibold">Schedule Name</label>
              <Input
                value={scheduleName}
                onChange={(e) => setScheduleName(e.target.value)}
                placeholder="e.g., Daily Backup Scan"
                maxLength={100}
              />
            </div>

            {/* Root Selection */}
            <div className="space-y-2">
              <label className="text-sm font-semibold">Root Directory</label>
              {preselectedRootId ? (
                <div className="flex items-center gap-2 px-3 py-2 border border-border rounded-md bg-muted">
                  <span className="text-sm">
                    {roots.find(r => r.root_id === preselectedRootId)?.root_path || 'Unknown root'}
                  </span>
                </div>
              ) : (
                <RootPicker
                  roots={roots}
                  value={selectedRootId}
                  onChange={setSelectedRootId}
                  placeholder="Select a root to scan"
                />
              )}
            </div>

            {/* Schedule Type */}
            <div className="space-y-4">
              <label className="text-sm font-semibold">Schedule Type</label>
              <div className="space-y-2">
                {(['Daily', 'Weekly', 'Monthly', 'Interval'] as ScheduleType[]).map((type) => (
                  <label key={type} className="flex items-center gap-2 cursor-pointer">
                    <input
                      type="radio"
                      name="schedule-type"
                      value={type}
                      checked={scheduleType === type}
                      onChange={(e) => setScheduleType(e.target.value as ScheduleType)}
                      className="w-4 h-4"
                    />
                    <span className="text-sm">{type}</span>
                  </label>
                ))}
              </div>
            </div>

            {/* Schedule-specific fields */}
            {(scheduleType === 'Daily' || scheduleType === 'Weekly' || scheduleType === 'Monthly') && (
              <div className="space-y-2">
                <label className="text-sm font-semibold">Time of Day</label>
                <Input
                  type="time"
                  value={timeOfDay}
                  onChange={(e) => setTimeOfDay(e.target.value)}
                  className="w-40"
                />
              </div>
            )}

            {scheduleType === 'Weekly' && (
              <div className="space-y-2">
                <label className="text-sm font-semibold">Days of Week</label>
                <div className="flex flex-wrap gap-2">
                  {DAYS_OF_WEEK.map((day) => (
                    <label key={day} className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={selectedDays.includes(day)}
                        onChange={() => toggleDay(day)}
                        className="w-4 h-4"
                      />
                      <span className="text-sm">{day}</span>
                    </label>
                  ))}
                </div>
              </div>
            )}

            {scheduleType === 'Monthly' && (
              <div className="space-y-2">
                <label className="text-sm font-semibold">Day of Month (1-31)</label>
                <Input
                  type="number"
                  min="1"
                  max="31"
                  value={dayOfMonth}
                  onChange={(e) => setDayOfMonth(parseInt(e.target.value) || 1)}
                  className="w-32"
                />
              </div>
            )}

            {scheduleType === 'Interval' && (
              <div className="space-y-2">
                <label className="text-sm font-semibold">Repeat Every</label>
                <div className="flex gap-2 items-center">
                  <Input
                    type="number"
                    min="1"
                    value={intervalValue}
                    onChange={(e) => setIntervalValue(parseInt(e.target.value) || 1)}
                    className="w-24"
                  />
                  <select
                    value={intervalUnit}
                    onChange={(e) => setIntervalUnit(e.target.value as IntervalUnit)}
                    className="h-10 px-3 border border-border rounded-md bg-background"
                  >
                    <option value="Minutes">Minutes</option>
                    <option value="Hours">Hours</option>
                    <option value="Days">Days</option>
                    <option value="Weeks">Weeks</option>
                  </select>
                </div>
              </div>
            )}

            {/* Scan Options */}
            <ScanOptionsFields
              hashMode={hashMode}
              validateMode={validateMode}
              onHashModeChange={setHashMode}
              onValidateModeChange={setValidateMode}
            />

            {/* Error Display */}
            {error && (
              <div className="rounded-md bg-red-50 p-3">
                <p className="text-sm text-red-600">{error}</p>
              </div>
            )}
          </div>
        )}

        <DialogFooter>
          <Button
            variant="outline"
            onClick={() => onOpenChange(false)}
            disabled={submitting}
          >
            Cancel
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={loading || roots.length === 0 || !selectedRootId || submitting}
          >
            {submitting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Creating...
              </>
            ) : (
              'Create Schedule'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
