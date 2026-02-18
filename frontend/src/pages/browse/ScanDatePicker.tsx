import { useState, useEffect, useRef } from 'react'
import { format } from 'date-fns'
import { Calendar as CalendarIcon } from 'lucide-react'
import { Button } from '@/components/ui/button'
import { Calendar } from '@/components/ui/calendar'
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from '@/components/ui/popover'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { cn } from '@/lib/utils'

interface ScanDatePickerProps {
  rootId: number
  onScanResolved: (scanId: number, startedAt: number) => void
  onNoScan: () => void
}

export function ScanDatePicker({ rootId, onScanResolved, onNoScan }: ScanDatePickerProps) {
  const [mode, setMode] = useState<'latest' | 'date'>('latest')
  const [selectedDate, setSelectedDate] = useState<Date | undefined>(undefined)
  const [resolvedInfo, setResolvedInfo] = useState<string | null>(null)
  const [resolving, setResolving] = useState(false)

  // Track last resolved request to avoid stale responses
  const resolveIdRef = useRef(0)

  // Resolve scan when rootId changes or mode/date changes
  useEffect(() => {
    const currentResolveId = ++resolveIdRef.current

    async function resolve() {
      setResolving(true)

      try {
        const params = new URLSearchParams({ root_id: rootId.toString() })

        if (mode === 'date' && selectedDate) {
          params.set('date', format(selectedDate, 'yyyy-MM-dd'))
        }

        const response = await fetch(`/api/scans/resolve?${params}`)

        // Check if this is still the latest request
        if (currentResolveId !== resolveIdRef.current) return

        if (response.ok) {
          const data = await response.json() as { scan_id: number; started_at: number }
          const scanDate = new Date(data.started_at * 1000)
          setResolvedInfo(`Scan #${data.scan_id} — ${format(scanDate, 'MMM d, yyyy h:mm a')}`)
          onScanResolved(data.scan_id, data.started_at)
        } else {
          setResolvedInfo('No completed scan found')
          onNoScan()
        }
      } catch {
        if (currentResolveId !== resolveIdRef.current) return
        setResolvedInfo('Failed to resolve scan')
        onNoScan()
      } finally {
        if (currentResolveId === resolveIdRef.current) {
          setResolving(false)
        }
      }
    }

    // Only resolve for date mode if a date is actually selected
    if (mode === 'latest' || (mode === 'date' && selectedDate)) {
      resolve()
    }
  }, [rootId, mode, selectedDate, onScanResolved, onNoScan])

  const handleModeChange = (value: string) => {
    setMode(value as 'latest' | 'date')
    if (value === 'latest') {
      setSelectedDate(undefined)
    }
  }

  const handleDateSelect = (date: Date | undefined) => {
    setSelectedDate(date)
  }

  return (
    <div className="flex items-center gap-3">
      <Select value={mode} onValueChange={handleModeChange}>
        <SelectTrigger className="w-[130px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="latest">Latest</SelectItem>
          <SelectItem value="date">Pick date</SelectItem>
        </SelectContent>
      </Select>

      {mode === 'date' && (
        <Popover>
          <PopoverTrigger asChild>
            <Button
              variant="outline"
              className={cn(
                'w-[160px] justify-start text-left font-normal',
                !selectedDate && 'text-muted-foreground'
              )}
            >
              <CalendarIcon className="mr-2 h-4 w-4" />
              {selectedDate ? format(selectedDate, 'MMM dd, yyyy') : 'Select date'}
            </Button>
          </PopoverTrigger>
          <PopoverContent className="w-auto p-0" align="start">
            <Calendar mode="single" selected={selectedDate} onSelect={handleDateSelect} />
          </PopoverContent>
        </Popover>
      )}

      {resolvedInfo && (
        <span className={cn(
          'text-xs',
          resolving ? 'text-muted-foreground animate-pulse' : 'text-muted-foreground'
        )}>
          {resolving ? 'Resolving...' : resolvedInfo}
        </span>
      )}
    </div>
  )
}
