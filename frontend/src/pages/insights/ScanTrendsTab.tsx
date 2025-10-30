import { useState, useEffect, useCallback } from 'react'
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
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import {
  ChartContainer,
  ChartTooltip,
  ChartTooltipContent,
} from '@/components/ui/chart'
import {
  AreaChart,
  Area,
  BarChart,
  Bar,
  LineChart,
  Line,
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
  scan_time: number // Unix timestamp
  file_count: number
  folder_count: number
  total_file_size: number
  alert_count: number
  modify_count: number
  add_count: number
  delete_count: number
}

export function ScanTrendsTab() {
  const [roots, setRoots] = useState<Root[]>([])
  const [selectedRootId, setSelectedRootId] = useState<string>('')
  const [fromDate, setFromDate] = useState<Date | undefined>()
  const [toDate, setToDate] = useState<Date | undefined>()
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [scanData, setScanData] = useState<ScanData[]>([])

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
      } catch (err) {
        console.error('Error loading roots:', err)
      }
    }
    loadRoots()
  }, [])

  // Load scan data when root or date range changes
  const loadScanData = useCallback(async () => {
    if (!selectedRootId) {
      setScanData([])
      return
    }

    try {
      setLoading(true)
      setError(null)

      // Build filters: root_id and only completed scans (state = 4)
      // Note: state is an int column, so we need >3 and <5 to match exactly 4
      const filters: Array<{ column: string; value: string }> = [
        { column: 'root_id', value: selectedRootId },
        { column: 'state', value: '>3' },  // state > 3
        { column: 'state', value: '<5' },  // state < 5 (combined = state 4)
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
        { name: 'total_file_size', visible: true, sort_direction: 'none', position: 4 },
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
        scan_time: parseInt(row[1]), // Position 1 now (scan_id is position 0)
        file_count: parseInt(row[2]) || 0,
        folder_count: parseInt(row[3]) || 0,
        total_file_size: parseInt(row[4]) || 0,
        alert_count: parseInt(row[5]) || 0,
        modify_count: parseInt(row[6]) || 0,
        add_count: parseInt(row[7]) || 0,
        delete_count: parseInt(row[8]) || 0,
      }))

      setScanData(data)

      // Auto-set date range to span of available data if not already set
      if (data.length > 0 && !fromDate && !toDate) {
        const minTime = Math.min(...data.map(d => d.scan_time))
        const maxTime = Math.max(...data.map(d => d.scan_time))
        setFromDate(new Date(minTime * 1000))
        setToDate(new Date(maxTime * 1000))
      }
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

  return (
    <div className="flex flex-col gap-6">
      {/* Control Toolbar */}
      <Card>
        <CardContent className="pt-6">
          <div className="flex items-center gap-4 flex-wrap">
            {/* Root Picker */}
            <div className="flex items-center gap-2">
              <label className="text-sm font-medium whitespace-nowrap">Root:</label>
              <Select value={selectedRootId} onValueChange={setSelectedRootId}>
                <SelectTrigger className="w-[300px]">
                  <SelectValue placeholder="Select a root to analyze" />
                </SelectTrigger>
                <SelectContent>
                  {roots.map((root) => (
                    <SelectItem key={root.root_id} value={root.root_id.toString()}>
                      {root.root_path}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>

            {/* From Date Picker */}
            <div className="flex items-center gap-2">
              <label className="text-sm font-medium whitespace-nowrap">From:</label>
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    variant="outline"
                    className={cn(
                      'w-[160px] justify-start text-left font-normal',
                      !fromDate && 'text-muted-foreground'
                    )}
                  >
                    <CalendarIcon className="mr-2 h-4 w-4" />
                    {fromDate ? format(fromDate, 'yyyy-MM-dd') : 'Pick a date'}
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-auto p-0" align="start">
                  <Calendar
                    mode="single"
                    selected={fromDate}
                    onSelect={setFromDate}
                    captionLayout="dropdown"
                  />
                </PopoverContent>
              </Popover>
            </div>

            {/* To Date Picker */}
            <div className="flex items-center gap-2">
              <label className="text-sm font-medium whitespace-nowrap">To:</label>
              <Popover>
                <PopoverTrigger asChild>
                  <Button
                    variant="outline"
                    className={cn(
                      'w-[160px] justify-start text-left font-normal',
                      !toDate && 'text-muted-foreground'
                    )}
                  >
                    <CalendarIcon className="mr-2 h-4 w-4" />
                    {toDate ? format(toDate, 'yyyy-MM-dd') : 'Pick a date'}
                  </Button>
                </PopoverTrigger>
                <PopoverContent className="w-auto p-0" align="start">
                  <Calendar
                    mode="single"
                    selected={toDate}
                    onSelect={setToDate}
                    captionLayout="dropdown"
                  />
                </PopoverContent>
              </Popover>
            </div>
          </div>
        </CardContent>
      </Card>

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
          {/* Total File Size Chart */}
          <Card>
            <CardHeader>
              <CardTitle>Total File Size</CardTitle>
            </CardHeader>
            <CardContent>
              <ChartContainer
                config={{
                  total_file_size: {
                    label: 'Total Size',
                    color: 'hsl(271 81% 56%)', // Vibrant purple
                  },
                }}
                className="aspect-auto h-[300px]"
              >
                <AreaChart
                  data={scanData.map((d) => ({
                    date: format(new Date(d.scan_time * 1000), 'MMM dd'),
                    total_file_size: d.total_file_size,
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
                  <Area
                    type="monotone"
                    dataKey="total_file_size"
                    stroke="var(--color-total_file_size)"
                    fill="var(--color-total_file_size)"
                    fillOpacity={0.6}
                    name="Total Size"
                  />
                </AreaChart>
              </ChartContainer>
            </CardContent>
          </Card>

          {/* File & Folder Counts Chart */}
          <Card>
            <CardHeader>
              <CardTitle>File & Folder Counts</CardTitle>
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
                <LineChart
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
                    tick={{ fill: 'hsl(var(--muted-foreground))' }}
                  />
                  <ChartTooltip content={<ChartTooltipContent />} />
                  <Legend />
                  <Line
                    type="monotone"
                    dataKey="file_count"
                    stroke="var(--color-file_count)"
                    strokeWidth={2}
                    dot={{ fill: 'var(--color-file_count)', r: 3 }}
                    activeDot={{ r: 5 }}
                    name="Files"
                  />
                  <Line
                    type="monotone"
                    dataKey="folder_count"
                    stroke="var(--color-folder_count)"
                    strokeWidth={2}
                    dot={{ fill: 'var(--color-folder_count)', r: 3 }}
                    activeDot={{ r: 5 }}
                    name="Folders"
                  />
                </LineChart>
              </ChartContainer>
            </CardContent>
          </Card>

          {/* Change Activity and Alerts - Side by side */}
          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <Card>
              <CardHeader>
                <CardTitle>Change Counts</CardTitle>
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
                    data={scanData.map((d) => ({
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
                      tick={{ fill: 'hsl(var(--muted-foreground))' }}
                    />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Legend />
                    <Bar dataKey="add_count" fill="var(--color-add_count)" name="Added" />
                    <Bar dataKey="modify_count" fill="var(--color-modify_count)" name="Modified" />
                    <Bar dataKey="delete_count" fill="var(--color-delete_count)" name="Deleted" />
                  </BarChart>
                </ChartContainer>
              </CardContent>
            </Card>

            <Card>
              <CardHeader>
                <CardTitle>Alerts Created</CardTitle>
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
                  <LineChart
                    data={scanData.map((d) => ({
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
                      tick={{ fill: 'hsl(var(--muted-foreground))' }}
                    />
                    <ChartTooltip content={<ChartTooltipContent />} />
                    <Legend />
                    <Line
                      type="monotone"
                      dataKey="alert_count"
                      stroke="var(--color-alert_count)"
                      strokeWidth={2}
                      dot={{ fill: 'var(--color-alert_count)', r: 3 }}
                      activeDot={{ r: 5 }}
                      name="Alerts"
                    />
                  </LineChart>
                </ChartContainer>
              </CardContent>
            </Card>
          </div>
        </div>
      )}
    </div>
  )
}
