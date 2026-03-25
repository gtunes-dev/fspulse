import { useState, useEffect, useCallback, useRef } from 'react'
import { useSearchParams, useNavigate } from 'react-router-dom'
import { format, subDays, subMonths, subYears, startOfDay } from 'date-fns'
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
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { RootCard } from '@/components/shared/RootCard'
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '@/components/ui/chart'
import {
  AreaChart,
  Area,
  LineChart,
  Line,
  BarChart,
  Bar,
  XAxis,
  YAxis,
  CartesianGrid,
  Legend,
} from 'recharts'
import { cn } from '@/lib/utils'
import { fetchQuery } from '@/lib/api'
import { useTaskContext } from '@/contexts/TaskContext'
import type { ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

interface ScanData {
  scan_id: number
  started_at: number // Unix timestamp
  file_count: number
  folder_count: number
  total_size: number
  modify_count: number
  add_count: number
  delete_count: number
  new_val_invalid_count: number
  val_invalid_count: number
  new_hash_suspect_count: number
  hash_suspect_count: number
}

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y' | 'custom'

export function TrendsPage() {
  const [searchParams, setSearchParams] = useSearchParams()
  const navigate = useNavigate()
  const { lastTaskCompletedAt } = useTaskContext()

  // Support deep-linking via URL params (e.g., from Dashboard root health card)
  const initialRootId = searchParams.get('root_id') || ''

  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>(initialRootId)

  // Sync with URL param changes (e.g., root carried via sidebar navigation)
  const urlRootId = searchParams.get('root_id') || ''
  useEffect(() => {
    if (urlRootId && urlRootId !== selectedRootId) {
      setSelectedRootId(urlRootId)
    }
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [urlRootId])

  // Update URL when root changes so sidebar can carry it to other pages
  const handleRootChange = useCallback((rootId: string) => {
    setSelectedRootId(rootId)
    if (rootId) {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev)
        next.set('root_id', rootId)
        return next
      }, { replace: true })
    } else {
      setSearchParams((prev) => {
        const next = new URLSearchParams(prev)
        next.delete('root_id')
        return next
      }, { replace: true })
    }
  }, [setSearchParams])
  const [timeWindow, setTimeWindow] = useState<TimeWindowPreset>('3m') // Default to 3 months
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [toDate, setToDate] = useState<Date | undefined>()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [scanData, setScanData] = useState<ScanData[]>([])
  const [hasAutoSelected, setHasAutoSelected] = useState(false)
  const [firstScanId, setFirstScanId] = useState<number | null>(null)
  const [excludeFirstScan, setExcludeFirstScan] = useState(false)
  const [hideEmptyChangeScans, setHideEmptyChangeScans] = useState(true)
  const [fromPickerOpen, setFromPickerOpen] = useState(false)
  const [toPickerOpen, setToPickerOpen] = useState(false)
  const [hiddenChangeSeries, setHiddenChangeSeries] = useState<Set<string>>(new Set(['unchanged_count']))
  const [hiddenValSeries, setHiddenValSeries] = useState<Set<string>>(new Set())
  const [hiddenHashSeries, setHiddenHashSeries] = useState<Set<string>>(new Set())
  const [hideEmptyValScans, setHideEmptyValScans] = useState(true)
  const [hideEmptyHashScans, setHideEmptyHashScans] = useState(true)

  const changesChartRef = useRef<HTMLDivElement>(null)
  const valChartRef = useRef<HTMLDivElement>(null)
  const hashChartRef = useRef<HTMLDivElement>(null)
  const hasDataRef = useRef(false)

  // Set pointer cursor only when hovering a data column (bar highlight active)
  const setCursorForChart = (ref: React.RefObject<HTMLDivElement | null>, pointer: boolean) => {
    const svg = ref.current?.querySelector('svg')
    if (svg) svg.style.cursor = pointer ? 'pointer' : ''
  }
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const handleBarChartMouseMove = (ref: React.RefObject<HTMLDivElement | null>) => (state: any) => {
    setCursorForChart(ref, state?.activeTooltipIndex != null)
  }
  const handleBarChartMouseLeave = (ref: React.RefObject<HTMLDivElement | null>) => () => {
    setCursorForChart(ref, false)
  }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const handleChartClick = (...args: any[]) => {
    for (const arg of args) {
      // Chart-level onClick: { activePayload: [{ payload: { scan_id } }] }
      const fromActivePayload = arg?.activePayload?.[0]?.payload?.scan_id
      // activeDot onClick: (event, payload) where scan_id is in payload.payload
      // Bar onClick: (data) where scan_id is in data.payload or data directly
      const scanId = fromActivePayload ?? arg?.payload?.scan_id ?? arg?.scan_id
      if (scanId) {
        navigate(`/browse?root_id=${selectedRootId}&scan_id=${scanId}`)
        return
      }
    }
  }

  // Calculate date range based on time window preset
  const getDateRangeForPreset = (preset: TimeWindowPreset): { from: Date; to: Date } => {
    const now = new Date()
    const today = startOfDay(now)

    switch (preset) {
      case '7d':
        return { from: subDays(today, 7), to: today }
      case '30d':
        return { from: subDays(today, 30), to: today }
      case '3m':
        return { from: subMonths(today, 3), to: today }
      case '6m':
        return { from: subMonths(today, 6), to: today }
      case '1y':
        return { from: subYears(today, 1), to: today }
      case 'custom':
        // For custom, just return current dates
        return {
          from: fromDate || subMonths(today, 3),
          to: toDate || today
        }
    }
  }

  // Update date range when time window changes
  useEffect(() => {
    if (timeWindow !== 'custom') {
      const { from, to } = getDateRangeForPreset(timeWindow)
      setFromDate(from)
      setToDate(to)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [timeWindow])

  // Load roots on mount
  useEffect(() => {
    async function loadRoots() {
      try {
        // Query for all roots
        const columns: ColumnSpec[] = [
          { name: 'root_id', visible: true, sort_direction: 'none', position: 0 },
          { name: 'root_path', visible: true, sort_direction: 'none', position: 1 },
        ]

        const response = await fetchQuery('roots', {
          columns,
          filters: [],
          limit: 1000, // Get all roots
          offset: 0,
        })

        const rootsData: Root[] = response.rows.map((row) => ({
          root_id: parseInt(row[0]),
          root_path: row[1],
        }))

        setRoots(rootsData)

        // Auto-select first root (unless deep-linked via URL) and set default date range
        if (rootsData.length > 0 && !hasAutoSelected) {
          if (!selectedRootId) {
            setSelectedRootId(rootsData[0].root_id.toString())
          }
          const today = startOfDay(new Date())
          setFromDate(subMonths(today, 3))
          setToDate(today)
          setHasAutoSelected(true)
        }
      } catch (err) {
        console.error('Error loading roots:', err)
      }
    }
    loadRoots()
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [hasAutoSelected])

  // Load the very first scan ID for this root
  useEffect(() => {
    async function loadFirstScan() {
      if (!selectedRootId) {
        setFirstScanId(null)
        return
      }

      try {
        // Query for the first completed scan for this root
        const columns: ColumnSpec[] = [
          { name: 'scan_id', visible: true, sort_direction: 'asc', position: 0 },
        ]

        const response = await fetchQuery('scans', {
          columns,
          filters: [
            { column: 'root_id', value: selectedRootId },
            { column: 'scan_state', value: 'C' },
          ],
          limit: 1,
          offset: 0,
        })

        if (response.rows.length > 0) {
          setFirstScanId(parseInt(response.rows[0][0]))
        } else {
          setFirstScanId(null)
        }
      } catch (err) {
        console.error('Error loading first scan:', err)
        setFirstScanId(null)
      }
    }
    loadFirstScan()
  }, [selectedRootId])

  // Load scan data when root or date range changes
  const loadScanData = useCallback(async () => {
    if (!selectedRootId) {
      hasDataRef.current = false
      setScanData([])
      return
    }

    // Guard: if both dates are set but from > to, just show empty
    if (fromDate && toDate && fromDate > toDate) {
      hasDataRef.current = false
      setScanData([])
      return
    }

    const isRefresh = hasDataRef.current

    try {
      // Only show the loading indicator for the initial load.
      // During refreshes (after task completion), keep existing charts
      // visible to avoid a flash of "Loading..." unmounting them.
      if (!isRefresh) {
        setLoading(true)
      }
      setError(null)

      // Build filters: root_id and only completed scans
      const filters: Array<{ column: string; value: string }> = [
        { column: 'root_id', value: selectedRootId },
        { column: 'scan_state', value: 'C' },  // Completed scans only
      ]

      // Add date range filter - DON'T include parentheses, backend adds them
      if (fromDate || toDate) {
        const fromStr = fromDate ? format(fromDate, 'yyyy-MM-dd') : '1970-01-01'
        const toStr = toDate ? format(toDate, 'yyyy-MM-dd') : '2099-12-31'
        filters.push({ column: 'started_at', value: `${fromStr}..${toStr}` })
      }

      // Query for scan data with all the fields we need
      // Start simple - just scan_id and started_at to test
      const columns: ColumnSpec[] = [
        { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
        { name: 'started_at', visible: true, sort_direction: 'asc', position: 1 },
        { name: 'file_count', visible: true, sort_direction: 'none', position: 2 },
        { name: 'folder_count', visible: true, sort_direction: 'none', position: 3 },
        { name: 'total_size', visible: true, sort_direction: 'none', position: 4 },
        { name: 'modify_count', visible: true, sort_direction: 'none', position: 5 },
        { name: 'add_count', visible: true, sort_direction: 'none', position: 6 },
        { name: 'delete_count', visible: true, sort_direction: 'none', position: 7 },
        { name: 'new_val_invalid_count', visible: true, sort_direction: 'none', position: 8 },
        { name: 'val_invalid_count', visible: true, sort_direction: 'none', position: 9 },
        { name: 'new_hash_suspect_count', visible: true, sort_direction: 'none', position: 10 },
        { name: 'hash_suspect_count', visible: true, sort_direction: 'none', position: 11 },
      ]

      const response = await fetchQuery('scans', {
        columns,
        filters,
        limit: 1000, // Get many scans for trend visualization
        offset: 0,
      })

      const data: ScanData[] = response.rows.map((row) => ({
        scan_id: parseInt(row[0]),
        started_at: parseInt(row[1]), // Position 1 now (scan_id is position 0)
        file_count: parseInt(row[2]) || 0,
        folder_count: parseInt(row[3]) || 0,
        total_size: parseInt(row[4]) || 0,
        modify_count: parseInt(row[5]) || 0,
        add_count: parseInt(row[6]) || 0,
        delete_count: parseInt(row[7]) || 0,
        new_val_invalid_count: parseInt(row[8]) || 0,
        val_invalid_count: parseInt(row[9]) || 0,
        new_hash_suspect_count: parseInt(row[10]) || 0,
        hash_suspect_count: parseInt(row[11]) || 0,
      }))

      hasDataRef.current = data.length > 0
      setScanData(data)
    } catch (err) {
      console.error('Error loading scan data:', err)
      // Treat API errors as empty results rather than showing an error message
      hasDataRef.current = false
      setScanData([])
    } finally {
      setLoading(false)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedRootId, fromDate, toDate, lastTaskCompletedAt])

  // Load data when filters change
  useEffect(() => {
    loadScanData()
  }, [loadScanData])

  const handleTimeWindowChange = (value: TimeWindowPreset) => {
    setTimeWindow(value)
    // If switching to custom, keep current dates
    // Otherwise, dates will be updated by the useEffect
  }

  // Check if first scan is in current data
  const firstScanInView = firstScanId !== null && scanData.some(d => d.scan_id === firstScanId)


  // Filter data for change count chart
  const changeCountData = scanData.filter(d => {
    if (excludeFirstScan && firstScanId !== null && d.scan_id === firstScanId) return false
    if (hideEmptyChangeScans) {
      const visibleKeys = ['add_count', 'modify_count', 'delete_count', 'unchanged_count'] as const
      const hasVisibleData = visibleKeys.some(key =>
        !hiddenChangeSeries.has(key) && (key === 'unchanged_count'
          ? (d.file_count + d.folder_count) - d.add_count - d.modify_count > 0
          : d[key] > 0)
      )
      if (!hasVisibleData) return false
    }
    return true
  })

  const valData = scanData.filter(d => {
    if (!hideEmptyValScans) return true
    const hasVisible =
      (!hiddenValSeries.has('new_val_invalid_count') && d.new_val_invalid_count > 0) ||
      (!hiddenValSeries.has('val_invalid_count') && d.val_invalid_count > 0)
    return hasVisible
  })

  const hashData = scanData.filter(d => {
    if (!hideEmptyHashScans) return true
    const hasVisible =
      (!hiddenHashSeries.has('new_hash_suspect_count') && d.new_hash_suspect_count > 0) ||
      (!hiddenHashSeries.has('hash_suspect_count') && d.hash_suspect_count > 0)
    return hasVisible
  })


  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Trends</h1>
      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={handleRootChange}
        actionBar={
          <>
            <Select value={timeWindow} onValueChange={handleTimeWindowChange}>
              <SelectTrigger className="w-[160px]">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="7d">Last 7 Days</SelectItem>
                <SelectItem value="30d">Last 30 Days</SelectItem>
                <SelectItem value="3m">Last 3 Months</SelectItem>
                <SelectItem value="6m">Last 6 Months</SelectItem>
                <SelectItem value="1y">Last Year</SelectItem>
                <SelectItem value="custom">Custom Range</SelectItem>
              </SelectContent>
            </Select>

            {/* Custom Date Pickers - Inline when selected */}
            {timeWindow === 'custom' && (
              <>
                <Popover open={fromPickerOpen} onOpenChange={setFromPickerOpen}>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className={cn(
                        'w-[160px] justify-start text-left font-normal',
                        !fromDate && 'text-muted-foreground'
                      )}
                    >
                      <CalendarIcon className="mr-2 h-4 w-4" />
                      {fromDate ? format(fromDate, 'd MMM yyyy') : 'From'}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="end">
                    <Calendar
                      mode="single"
                      selected={fromDate}
                      onSelect={(date) => { setFromDate(date); setFromPickerOpen(false) }}
                      defaultMonth={fromDate}
                      captionLayout="dropdown"
                    />
                  </PopoverContent>
                </Popover>

                <span className="text-muted-foreground">to</span>

                <Popover open={toPickerOpen} onOpenChange={setToPickerOpen}>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className={cn(
                        'w-[160px] justify-start text-left font-normal',
                        !toDate && 'text-muted-foreground'
                      )}
                    >
                      <CalendarIcon className="mr-2 h-4 w-4" />
                      {toDate ? format(toDate, 'd MMM yyyy') : 'To'}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="end">
                    <Calendar
                      mode="single"
                      selected={toDate}
                      onSelect={(date) => { setToDate(date); setToPickerOpen(false) }}
                      defaultMonth={toDate}
                      captionLayout="dropdown"
                    />
                  </PopoverContent>
                </Popover>
              </>
            )}
          </>
        }
      >
        {/* Chart Area */}
        {!selectedRootId ? (
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Select a root to view scan trends
          </div>
        ) : loading ? (
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Loading scan data...
          </div>
        ) : error ? (
          <div className="flex items-center justify-center h-64 text-red-600">
            {error}
          </div>
        ) : scanData.length === 0 ? (
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            No scans for &lsquo;{roots.find(r => r.root_id.toString() === selectedRootId)?.root_path ?? 'Unknown'}&rsquo;{fromDate && toDate ? ` between ${format(fromDate, 'd MMM yyyy')} and ${format(toDate, 'd MMM yyyy')}` : ''}
          </div>
        ) : (
          <div className="grid grid-cols-1 gap-6">
            {/* Total Size Chart */}
            <Card>
              <CardHeader>
                <CardTitle>Total Size</CardTitle>
              </CardHeader>
              <CardContent>
                <ChartContainer
                  config={{
                    total_size: {
                      label: 'Total Size',
                      color: 'hsl(271 81% 56%)', // Vibrant purple
                    },
                  }}
                  className="aspect-auto h-[300px]"
                >
                  <LineChart
                    data={scanData.map((d) => ({
                      date: format(new Date(d.started_at * 1000), 'MMM dd'),
                      total_size: d.total_size,
                      scan_id: d.scan_id,
                    }))}
                  >
                    <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                    <XAxis
                      dataKey="date"
                      tick={{ fill: 'hsl(var(--muted-foreground))' }}
                    />
                    <YAxis
                      tick={{ fill: 'hsl(var(--muted-foreground))' }}
                      tickFormatter={(value) => {
                        const bytes = value as number
                        const units = ['B', 'KB', 'MB', 'GB', 'TB']
                        let i = 0
                        let size = bytes
                        while (size >= 1024 && i < units.length - 1) {
                          size /= 1024
                          i++
                        }
                        return `${size.toFixed(1)} ${units[i]}`
                      }}
                    />
                    <ChartTooltip
                      content={<ChartTooltipContent />}
                      formatter={(value) => {
                        const bytes = value as number
                        const units = ['B', 'KB', 'MB', 'GB', 'TB']
                        let i = 0
                        let size = bytes
                        while (size >= 1024 && i < units.length - 1) {
                          size /= 1024
                          i++
                        }
                        return `${size.toFixed(2)} ${units[i]}`
                      }}
                    />
                    <Legend />
                    <Line
                      type="step"
                      dataKey="total_size"
                      stroke="var(--color-total_size)"
                      strokeWidth={2}
                      dot={false}
                      activeDot={{ r: 5, cursor: 'pointer', onClick: handleChartClick }}
                      name="Total Size"
                    />
                  </LineChart>
                </ChartContainer>
              </CardContent>
            </Card>

            {/* Items and Changes - Side by side */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              {/* Item Count Chart */}
              <Card>
                <CardHeader>
                  <CardTitle>Items</CardTitle>
                </CardHeader>
                <CardContent>
                  <ChartContainer
                    config={{
                      file_count: {
                        label: 'Files',
                        color: 'hsl(221 83% 53%)',
                      },
                      folder_count: {
                        label: 'Folders',
                        color: 'hsl(142 71% 45%)',
                      },
                    }}
                    className="aspect-auto h-[300px]"
                  >
                    <AreaChart
                      data={scanData.map((d) => ({
                        date: format(new Date(d.started_at * 1000), 'MMM dd'),
                        file_count: d.file_count,
                        folder_count: d.folder_count,
                        scan_id: d.scan_id,
                      }))}
                    >
                      <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                      <XAxis dataKey="date" tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                      <YAxis allowDecimals={false} tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                      <ChartTooltip content={<ChartTooltipContent />} />
                      <Legend />
                      <Area type="monotone" dataKey="file_count" stackId="1" stroke="var(--color-file_count)" fill="var(--color-file_count)" fillOpacity={0.6} activeDot={{ r: 5, cursor: 'pointer', onClick: handleChartClick }} name="Files" />
                      <Area type="monotone" dataKey="folder_count" stackId="1" stroke="var(--color-folder_count)" fill="var(--color-folder_count)" fillOpacity={0.6} activeDot={{ r: 5, cursor: 'pointer', onClick: handleChartClick }} name="Folders" />
                    </AreaChart>
                  </ChartContainer>
                </CardContent>
              </Card>
              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-2">
                  <CardTitle>Changes</CardTitle>
                  <div className="flex items-center gap-4">
                    {firstScanInView && (
                      <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                        <input
                          type="checkbox"
                          checked={excludeFirstScan}
                          onChange={(e) => setExcludeFirstScan(e.target.checked)}
                          className="cursor-pointer"
                        />
                        <span className="text-muted-foreground">Hide first scan</span>
                      </label>
                    )}
                    <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                      <input
                        type="checkbox"
                        checked={hideEmptyChangeScans}
                        onChange={(e) => setHideEmptyChangeScans(e.target.checked)}
                        className="cursor-pointer"
                      />
                      <span className="text-muted-foreground">Hide empty</span>
                    </label>
                  </div>
                </CardHeader>
                <CardContent>
                  {changeCountData.length === 0 ? (
                    <div className="flex items-center justify-center h-[300px] text-muted-foreground">
                      No scans to display
                    </div>
                  ) : (
                  <div ref={changesChartRef}>
                  <ChartContainer
                    config={{
                      add_count: {
                        label: 'Added',
                        color: 'hsl(142 71% 45%)',
                      },
                      modify_count: {
                        label: 'Modified',
                        color: 'hsl(217 91% 60%)',
                      },
                      delete_count: {
                        label: 'Deleted',
                        color: 'hsl(0 84% 60%)',
                      },
                      unchanged_count: {
                        label: 'Unchanged',
                        color: 'hsl(220 9% 60%)',
                      },
                    }}
                    className="aspect-auto h-[300px]"
                  >
                    <BarChart
                      data={changeCountData.map((d) => ({
                        date: format(new Date(d.started_at * 1000), 'MMM dd'),
                        add_count: d.add_count,
                        modify_count: d.modify_count,
                        delete_count: d.delete_count,
                        unchanged_count: (d.file_count + d.folder_count) - d.add_count - d.modify_count,
                        scan_id: d.scan_id,
                      }))}
                      onClick={handleChartClick}
                      onMouseMove={handleBarChartMouseMove(changesChartRef)}
                      onMouseLeave={handleBarChartMouseLeave(changesChartRef)}
                    >
                      <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                      <XAxis
                        dataKey="date"
                        tick={{ fill: 'hsl(var(--muted-foreground))' }}
                      />
                      <YAxis
                        allowDecimals={false}
                        tick={{ fill: 'hsl(var(--muted-foreground))' }}
                      />
                      <ChartTooltip content={<ChartTooltipContent />} />
                      <Legend content={() => (
                        <div className="flex items-center justify-center gap-1 pt-1">
                          {([
                            { key: 'add_count', label: 'Added', color: 'bg-green-500', ring: 'ring-green-500/40' },
                            { key: 'modify_count', label: 'Modified', color: 'bg-blue-500', ring: 'ring-blue-500/40' },
                            { key: 'delete_count', label: 'Deleted', color: 'bg-red-500', ring: 'ring-red-500/40' },
                            { key: 'unchanged_count', label: 'Unchanged', color: 'bg-gray-400', ring: 'ring-gray-400/40' },
                          ]).map(({ key, label, color, ring }) => {
                            const visible = !hiddenChangeSeries.has(key)
                            return (
                              <button
                                key={key}
                                className={cn(
                                  'inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-xs cursor-pointer transition-colors text-left',
                                  visible
                                    ? 'text-foreground hover:bg-accent'
                                    : 'text-muted-foreground/40 hover:bg-accent/50'
                                )}
                                onClick={(e) => { e.stopPropagation(); setHiddenChangeSeries(prev => {
                                  const next = new Set(prev)
                                  if (next.has(key)) next.delete(key)
                                  else next.add(key)
                                  return next
                                })}}
                              >
                                <span
                                  className={cn(
                                    'inline-block w-3 h-3 rounded-full transition-all flex-shrink-0',
                                    visible
                                      ? `${color} ring-2 ${ring}`
                                      : 'bg-transparent ring-1 ring-muted-foreground/25'
                                  )}
                                />
                                {label}
                              </button>
                            )
                          })}
                        </div>
                      )} />
                      {/* Stacked bars - conditionally rendered based on legend toggles */}
                      {!hiddenChangeSeries.has('add_count') && (
                        <Bar dataKey="add_count" stackId="a" fill="var(--color-add_count)" name="Added" />
                      )}
                      {!hiddenChangeSeries.has('modify_count') && (
                        <Bar dataKey="modify_count" stackId="a" fill="var(--color-modify_count)" name="Modified" />
                      )}
                      {!hiddenChangeSeries.has('delete_count') && (
                        <Bar dataKey="delete_count" stackId="a" fill="var(--color-delete_count)" name="Deleted" />
                      )}
                      {!hiddenChangeSeries.has('unchanged_count') && (
                        <Bar dataKey="unchanged_count" stackId="a" fill="var(--color-unchanged_count)" name="Unchanged" />
                      )}
                    </BarChart>
                  </ChartContainer>
                  </div>
                  )}
                </CardContent>
              </Card>

            </div>

            {/* Integrity - Validation and Hashes - Side by side */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              {/* Validation Chart */}
              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-2">
                  <CardTitle>Validation Errors</CardTitle>
                  <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                    <input type="checkbox" checked={hideEmptyValScans} onChange={(e) => setHideEmptyValScans(e.target.checked)} className="cursor-pointer" />
                    <span className="text-muted-foreground">Hide empty</span>
                  </label>
                </CardHeader>
                <CardContent>
                  {valData.length === 0 ? (
                    <div className="flex items-center justify-center h-[300px] text-muted-foreground">No scans to display</div>
                  ) : (
                    <div ref={valChartRef}>
                      <ChartContainer
                        config={{
                          new_val_invalid_count: { label: 'New Errors', color: 'hsl(347 77% 50%)' },
                          val_invalid_count: { label: 'Total Errors', color: 'hsl(347 77% 70%)' },
                        }}
                        className="aspect-auto h-[300px]"
                      >
                        <BarChart
                          data={valData.map((d) => ({
                            date: format(new Date(d.started_at * 1000), 'MMM dd'),
                            new_val_invalid_count: d.new_val_invalid_count,
                            val_invalid_count: d.val_invalid_count,
                            scan_id: d.scan_id,
                          }))}
                          onClick={handleChartClick}
                          onMouseMove={handleBarChartMouseMove(valChartRef)}
                          onMouseLeave={handleBarChartMouseLeave(valChartRef)}
                        >
                          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                          <XAxis dataKey="date" tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                          <YAxis allowDecimals={false} tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                          <ChartTooltip content={<ChartTooltipContent />} />
                          <Legend content={() => (
                            <div className="flex items-center justify-center gap-1 pt-1">
                              {([
                                { key: 'new_val_invalid_count', label: 'New Errors', color: 'bg-rose-500', ring: 'ring-rose-500/40' },
                                { key: 'val_invalid_count', label: 'Total Errors', color: 'bg-rose-300', ring: 'ring-rose-300/40' },
                              ]).map(({ key, label, color, ring }) => {
                                const visible = !hiddenValSeries.has(key)
                                return (
                                  <button
                                    key={key}
                                    className={cn(
                                      'inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-xs cursor-pointer transition-colors text-left',
                                      visible ? 'text-foreground hover:bg-accent' : 'text-muted-foreground/40 hover:bg-accent/50'
                                    )}
                                    onClick={(e) => { e.stopPropagation(); setHiddenValSeries(prev => {
                                      const next = new Set(prev)
                                      if (next.has(key)) next.delete(key); else next.add(key)
                                      return next
                                    })}}
                                  >
                                    <span className={cn('inline-block w-3 h-3 rounded-full transition-all flex-shrink-0', visible ? `${color} ring-2 ${ring}` : 'bg-transparent ring-1 ring-muted-foreground/25')} />
                                    {label}
                                  </button>
                                )
                              })}
                            </div>
                          )} />
                          {!hiddenValSeries.has('new_val_invalid_count') && (
                            <Bar dataKey="new_val_invalid_count" fill="var(--color-new_val_invalid_count)" name="New Errors" />
                          )}
                          {!hiddenValSeries.has('val_invalid_count') && (
                            <Bar dataKey="val_invalid_count" fill="var(--color-val_invalid_count)" name="Total Errors" />
                          )}
                        </BarChart>
                      </ChartContainer>
                    </div>
                  )}
                </CardContent>
              </Card>

              {/* Hashes Chart */}
              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-2">
                  <CardTitle>Suspicious Hashes</CardTitle>
                  <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                    <input type="checkbox" checked={hideEmptyHashScans} onChange={(e) => setHideEmptyHashScans(e.target.checked)} className="cursor-pointer" />
                    <span className="text-muted-foreground">Hide empty</span>
                  </label>
                </CardHeader>
                <CardContent>
                  {hashData.length === 0 ? (
                    <div className="flex items-center justify-center h-[300px] text-muted-foreground">No scans to display</div>
                  ) : (
                    <div ref={hashChartRef}>
                      <ChartContainer
                        config={{
                          new_hash_suspect_count: { label: 'New Suspicious', color: 'hsl(38 92% 50%)' },
                          hash_suspect_count: { label: 'Total Suspicious', color: 'hsl(38 92% 70%)' },
                        }}
                        className="aspect-auto h-[300px]"
                      >
                        <BarChart
                          data={hashData.map((d) => ({
                            date: format(new Date(d.started_at * 1000), 'MMM dd'),
                            new_hash_suspect_count: d.new_hash_suspect_count,
                            hash_suspect_count: d.hash_suspect_count,
                            scan_id: d.scan_id,
                          }))}
                          onClick={handleChartClick}
                          onMouseMove={handleBarChartMouseMove(hashChartRef)}
                          onMouseLeave={handleBarChartMouseLeave(hashChartRef)}
                        >
                          <CartesianGrid strokeDasharray="3 3" className="stroke-muted" />
                          <XAxis dataKey="date" tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                          <YAxis allowDecimals={false} tick={{ fill: 'hsl(var(--muted-foreground))' }} />
                          <ChartTooltip content={<ChartTooltipContent />} />
                          <Legend content={() => (
                            <div className="flex items-center justify-center gap-1 pt-1">
                              {([
                                { key: 'new_hash_suspect_count', label: 'New Suspicious', color: 'bg-amber-500', ring: 'ring-amber-500/40' },
                                { key: 'hash_suspect_count', label: 'Total Suspicious', color: 'bg-amber-300', ring: 'ring-amber-300/40' },
                              ]).map(({ key, label, color, ring }) => {
                                const visible = !hiddenHashSeries.has(key)
                                return (
                                  <button
                                    key={key}
                                    className={cn(
                                      'inline-flex items-center gap-1.5 px-2 py-0.5 rounded text-xs cursor-pointer transition-colors text-left',
                                      visible ? 'text-foreground hover:bg-accent' : 'text-muted-foreground/40 hover:bg-accent/50'
                                    )}
                                    onClick={(e) => { e.stopPropagation(); setHiddenHashSeries(prev => {
                                      const next = new Set(prev)
                                      if (next.has(key)) next.delete(key); else next.add(key)
                                      return next
                                    })}}
                                  >
                                    <span className={cn('inline-block w-3 h-3 rounded-full transition-all flex-shrink-0', visible ? `${color} ring-2 ${ring}` : 'bg-transparent ring-1 ring-muted-foreground/25')} />
                                    {label}
                                  </button>
                                )
                              })}
                            </div>
                          )} />
                          {!hiddenHashSeries.has('new_hash_suspect_count') && (
                            <Bar dataKey="new_hash_suspect_count" fill="var(--color-new_hash_suspect_count)" name="New Suspicious" />
                          )}
                          {!hiddenHashSeries.has('hash_suspect_count') && (
                            <Bar dataKey="hash_suspect_count" fill="var(--color-hash_suspect_count)" name="Total Suspicious" />
                          )}
                        </BarChart>
                      </ChartContainer>
                    </div>
                  )}
                </CardContent>
              </Card>
            </div>
          </div>
        )}
      </RootCard>
    </div>
  )
}
