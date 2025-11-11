import { useState, useEffect, useCallback } from 'react'
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
import type { ColumnSpec } from '@/lib/types'

interface Root {
  root_id: number
  root_path: string
}

interface ScanData {
  scan_id: number
  scan_time: number // Unix timestamp
  file_count: number
  folder_count: number
  total_size: number
  alert_count: number
  modify_count: number
  add_count: number
  delete_count: number
}

type TimeWindowPreset = '7d' | '30d' | '3m' | '6m' | '1y' | 'custom'

export function InsightsPage() {
  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [timeWindow, setTimeWindow] = useState<TimeWindowPreset>('3m') // Default to 3 months
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [toDate, setToDate] = useState<Date | undefined>()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [scanData, setScanData] = useState<ScanData[]>([])
  const [hasAutoSelected, setHasAutoSelected] = useState(false)
  const [firstScanId, setFirstScanId] = useState<number | null>(null)
  const [firstValidatingScanId, setFirstValidatingScanId] = useState<number | null>(null)
  const [excludeFirstScan, setExcludeFirstScan] = useState(false)
  const [excludeFirstValidatingScan, setExcludeFirstValidatingScan] = useState(false)

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

        // Auto-select first root and set default date range
        if (rootsData.length > 0 && !hasAutoSelected) {
          setSelectedRootId(rootsData[0].root_id.toString())
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
  }, [hasAutoSelected])

  // Load the very first scan ID for this root
  useEffect(() => {
    async function loadFirstScan() {
      if (!selectedRootId) {
        setFirstScanId(null)
        setFirstValidatingScanId(null)
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

        // Query for the first validating scan (is_val = true)
        const validatingResponse = await fetchQuery('scans', {
          columns,
          filters: [
            { column: 'root_id', value: selectedRootId },
            { column: 'scan_state', value: 'C' },
            { column: 'is_val', value: 'true' },
          ],
          limit: 1,
          offset: 0,
        })

        if (validatingResponse.rows.length > 0) {
          setFirstValidatingScanId(parseInt(validatingResponse.rows[0][0]))
        } else {
          setFirstValidatingScanId(null)
        }
      } catch (err) {
        console.error('Error loading first scan:', err)
        setFirstScanId(null)
        setFirstValidatingScanId(null)
      }
    }
    loadFirstScan()
  }, [selectedRootId])

  // Load scan data when root or date range changes
  const loadScanData = useCallback(async () => {
    if (!selectedRootId) {
      setScanData([])
      return
    }

    try {
      setLoading(true)
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
        filters.push({ column: 'scan_time', value: `${fromStr}..${toStr}` })  // No () !
      }

      // Query for scan data with all the fields we need
      // Start simple - just scan_id and scan_time to test
      const columns: ColumnSpec[] = [
        { name: 'scan_id', visible: true, sort_direction: 'none', position: 0 },
        { name: 'scan_time', visible: true, sort_direction: 'asc', position: 1 },
        { name: 'file_count', visible: true, sort_direction: 'none', position: 2 },
        { name: 'folder_count', visible: true, sort_direction: 'none', position: 3 },
        { name: 'total_size', visible: true, sort_direction: 'none', position: 4 },
        { name: 'alert_count', visible: true, sort_direction: 'none', position: 5 },
        { name: 'modify_count', visible: true, sort_direction: 'none', position: 6 },
        { name: 'add_count', visible: true, sort_direction: 'none', position: 7 },
        { name: 'delete_count', visible: true, sort_direction: 'none', position: 8 },
      ]

      const response = await fetchQuery('scans', {
        columns,
        filters,
        limit: 1000, // Get many scans for trend visualization
        offset: 0,
      })

      const data: ScanData[] = response.rows.map((row) => ({
        scan_id: parseInt(row[0]),
        scan_time: parseInt(row[1]), // Position 1 now (scan_id is position 0)
        file_count: parseInt(row[2]) || 0,
        folder_count: parseInt(row[3]) || 0,
        total_size: parseInt(row[4]) || 0,
        alert_count: parseInt(row[5]) || 0,
        modify_count: parseInt(row[6]) || 0,
        add_count: parseInt(row[7]) || 0,
        delete_count: parseInt(row[8]) || 0,
      }))

      setScanData(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to load scan data')
      console.error('Error loading scan data:', err)
    } finally {
      setLoading(false)
    }
  }, [selectedRootId, fromDate, toDate])

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
  const firstValidatingScanInView = firstValidatingScanId !== null && scanData.some(d => d.scan_id === firstValidatingScanId)

  // Filter data for change count chart
  const changeCountData = excludeFirstScan && firstScanId !== null
    ? scanData.filter(d => d.scan_id !== firstScanId)
    : scanData

  // Filter data for alerts chart
  const alertsData = excludeFirstValidatingScan && firstValidatingScanId !== null
    ? scanData.filter(d => d.scan_id !== firstValidatingScanId)
    : scanData

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Insights</h1>
      <RootCard
        roots={roots}
        selectedRootId={selectedRootId}
        onRootChange={setSelectedRootId}
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
                <Popover>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className={cn(
                        'w-[140px] justify-start text-left font-normal',
                        !fromDate && 'text-muted-foreground'
                      )}
                    >
                      <CalendarIcon className="mr-2 h-4 w-4" />
                      {fromDate ? format(fromDate, 'MMM dd') : 'From'}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="end">
                    <Calendar
                      mode="single"
                      selected={fromDate}
                      onSelect={setFromDate}
                      captionLayout="dropdown"
                    />
                  </PopoverContent>
                </Popover>

                <span className="text-muted-foreground">to</span>

                <Popover>
                  <PopoverTrigger asChild>
                    <Button
                      variant="outline"
                      className={cn(
                        'w-[140px] justify-start text-left font-normal',
                        !toDate && 'text-muted-foreground'
                      )}
                    >
                      <CalendarIcon className="mr-2 h-4 w-4" />
                      {toDate ? format(toDate, 'MMM dd') : 'To'}
                    </Button>
                  </PopoverTrigger>
                  <PopoverContent className="w-auto p-0" align="end">
                    <Calendar
                      mode="single"
                      selected={toDate}
                      onSelect={setToDate}
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
            No scan data available for this root
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
                      date: format(new Date(d.scan_time * 1000), 'MMM dd'),
                      total_size: d.total_size,
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
                      name="Total Size"
                    />
                  </LineChart>
                </ChartContainer>
              </CardContent>
            </Card>

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
                      color: 'hsl(221 83% 53%)', // Vibrant blue
                    },
                    folder_count: {
                      label: 'Folders',
                      color: 'hsl(142 71% 45%)', // Vibrant green
                    },
                  }}
                  className="aspect-auto h-[300px]"
                >
                  <AreaChart
                    data={scanData.map((d) => ({
                      date: format(new Date(d.scan_time * 1000), 'MMM dd'),
                      file_count: d.file_count,
                      folder_count: d.folder_count,
                    }))}
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
                    <Legend />
                    {/* Files as base layer */}
                    <Area
                      type="monotone"
                      dataKey="file_count"
                      stackId="1"
                      stroke="var(--color-file_count)"
                      fill="var(--color-file_count)"
                      fillOpacity={0.6}
                      name="Files"
                    />
                    {/* Folders stacked on top */}
                    <Area
                      type="monotone"
                      dataKey="folder_count"
                      stackId="1"
                      stroke="var(--color-folder_count)"
                      fill="var(--color-folder_count)"
                      fillOpacity={0.6}
                      name="Folders"
                    />
                  </AreaChart>
                </ChartContainer>
              </CardContent>
            </Card>

            {/* Change Activity and Alerts - Side by side */}
            <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-2">
                  <CardTitle>Changes</CardTitle>
                  {firstScanInView && (
                    <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                      <input
                        type="checkbox"
                        checked={excludeFirstScan}
                        onChange={(e) => setExcludeFirstScan(e.target.checked)}
                        className="cursor-pointer"
                      />
                      <span className="text-muted-foreground">Exclude initial baseline scan</span>
                    </label>
                  )}
                </CardHeader>
                <CardContent>
                  <ChartContainer
                    config={{
                      add_count: {
                        label: 'Added',
                        color: 'hsl(142 71% 45%)', // Vibrant green
                      },
                      modify_count: {
                        label: 'Modified',
                        color: 'hsl(45 93% 47%)', // Vibrant amber
                      },
                      delete_count: {
                        label: 'Deleted',
                        color: 'hsl(0 84% 60%)', // Vibrant red
                      },
                    }}
                    className="aspect-auto h-[300px]"
                  >
                    <BarChart
                      data={changeCountData.map((d) => ({
                        date: format(new Date(d.scan_time * 1000), 'MMM dd'),
                        add_count: d.add_count,
                        modify_count: d.modify_count,
                        delete_count: d.delete_count,
                      }))}
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
                      <Legend />
                      {/* Stacked bars - all on same vertical bar */}
                      <Bar dataKey="add_count" stackId="a" fill="var(--color-add_count)" name="Added" />
                      <Bar dataKey="modify_count" stackId="a" fill="var(--color-modify_count)" name="Modified" />
                      <Bar dataKey="delete_count" stackId="a" fill="var(--color-delete_count)" name="Deleted" />
                    </BarChart>
                  </ChartContainer>
                </CardContent>
              </Card>

              <Card>
                <CardHeader className="flex flex-row items-center justify-between pb-2">
                  <CardTitle>New Alerts</CardTitle>
                  {firstValidatingScanInView && (
                    <label className="flex items-center gap-2 text-sm font-normal cursor-pointer">
                      <input
                        type="checkbox"
                        checked={excludeFirstValidatingScan}
                        onChange={(e) => setExcludeFirstValidatingScan(e.target.checked)}
                        className="cursor-pointer"
                      />
                      <span className="text-muted-foreground">Exclude initial baseline scan</span>
                    </label>
                  )}
                </CardHeader>
                <CardContent>
                  <ChartContainer
                    config={{
                      alert_count: {
                        label: 'Alerts',
                        color: 'hsl(24 95% 53%)', // Vibrant orange
                      },
                    }}
                    className="aspect-auto h-[300px]"
                  >
                    <BarChart
                      data={alertsData.map((d) => ({
                        date: format(new Date(d.scan_time * 1000), 'MMM dd'),
                        alert_count: d.alert_count,
                      }))}
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
                      <Legend />
                      <Bar
                        dataKey="alert_count"
                        fill="var(--color-alert_count)"
                        name="Alerts"
                      />
                    </BarChart>
                  </ChartContainer>
                </CardContent>
              </Card>
            </div>
          </div>
        )}
      </RootCard>
    </div>
  )
}
