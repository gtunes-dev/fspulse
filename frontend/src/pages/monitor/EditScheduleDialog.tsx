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
import { ScanOptionsFields } from '@/components/shared/ScanOptionsFields'
import { Loader2 } from 'lucide-react'
import type { ScheduleWithRoot, ScheduleType, IntervalUnit } from '@/lib/types'

interface EditScheduleDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  schedule: ScheduleWithRoot | null
  onSuccess?: () => void
}

const DAYS_OF_WEEK = ['Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat', 'Sun']

export function EditScheduleDialog({
  open,
  onOpenChange,
  schedule,
  onSuccess,
}: EditScheduleDialogProps) {
  const [scheduleName, setScheduleName] = useState<string>('')
  const [scheduleType, setScheduleType] = useState<ScheduleType>('Daily')
  const [timeOfDay, setTimeOfDay] = useState<string>('09:00')
  const [selectedDays, setSelectedDays] = useState<string[]>(['Mon', 'Wed', 'Fri'])
  const [dayOfMonth, setDayOfMonth] = useState<number>(1)
  const [intervalValue, setIntervalValue] = useState<number>(2)
  const [intervalUnit, setIntervalUnit] = useState<IntervalUnit>('Hours')
  const [hashMode, setHashMode] = useState<string>('All')
  const [validateMode, setValidateMode] = useState<string>('New or Changed')

  const [submitting, setSubmitting] = useState(false)
  const [error, setError] = useState<string | null>(null)

  // Load schedule data when dialog opens
  useEffect(() => {
    if (open && schedule) {
      setScheduleName(schedule.schedule_name)
      setScheduleType(schedule.schedule_type)
      setTimeOfDay(schedule.time_of_day || '09:00')

      // Parse days of week if present
      if (schedule.days_of_week) {
        try {
          setSelectedDays(JSON.parse(schedule.days_of_week))
        } catch {
          setSelectedDays(['Mon', 'Wed', 'Fri'])
        }
      }

      setDayOfMonth(schedule.day_of_month || 1)
      setIntervalValue(schedule.interval_value || 2)
      setIntervalUnit(schedule.interval_unit || 'Hours')

      // Map backend modes to UI modes
      const mapModeToUI = (mode: string): string => {
        if (mode === 'New') return 'New or Changed'
        return mode // 'All' and 'None' map directly
      }

      setHashMode(mapModeToUI(schedule.hash_mode))
      setValidateMode(mapModeToUI(schedule.validate_mode))
      setError(null)
    }
  }, [open, schedule])

  function toggleDay(day: string) {
    setSelectedDays(prev =>
      prev.includes(day) ? prev.filter(d => d !== day) : [...prev, day]
    )
  }

  async function handleSubmit() {
    if (!schedule) return

    // Validation
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

      // Update the schedule
      const response = await fetch(`/api/schedules/${schedule.schedule_id}`, {
        method: 'PUT',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(requestBody),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(`Failed to update schedule: ${errorText}`)
      }

      // Success callback
      onSuccess?.()

      // Close dialog
      onOpenChange(false)
    } catch (err) {
      console.error('Error updating schedule:', err)
      setError(err instanceof Error ? err.message : 'Failed to update schedule')
    } finally {
      setSubmitting(false)
    }
  }

  if (!schedule) return null

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-[550px] max-h-[90vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Edit Schedule</DialogTitle>
          <DialogDescription>
            Modify the schedule settings for this recurring scan
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-6">
          {/* Root Path (read-only) */}
          <div className="space-y-2">
            <label className="text-sm font-semibold">Root Directory</label>
            <div className="bg-muted rounded-md p-3">
              <div className="text-sm font-mono">{schedule.root_path}</div>
            </div>
          </div>

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
            disabled={submitting}
          >
            {submitting ? (
              <>
                <Loader2 className="h-4 w-4 mr-2 animate-spin" />
                Saving...
              </>
            ) : (
              'Save Changes'
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
