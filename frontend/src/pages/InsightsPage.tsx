import { useState } from 'react'
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Input } from '@/components/ui/input'
import { AlertsTab } from './insights/AlertsTab'
import type { ContextFilterType } from '@/lib/types'

export function InsightsPage() {
  const [contextFilter, setContextFilter] = useState<ContextFilterType>('all')
  const [contextValue, setContextValue] = useState('')

  const handleContextFilterChange = (value: ContextFilterType) => {
    setContextFilter(value)
    setContextValue('')
  }

  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Insights</h1>

      <Tabs defaultValue="alerts" className="flex-1 flex flex-col">
        <TabsList>
          <TabsTrigger value="alerts">Alerts</TabsTrigger>
          <TabsTrigger value="statistics">Statistics</TabsTrigger>
          <TabsTrigger value="changes">Changes</TabsTrigger>
        </TabsList>

        {/* Context Filter Toolbar */}
        <div className="flex items-center gap-4 py-4 px-4 bg-muted/30 rounded-lg mb-4">
          <label className="text-sm font-medium">Context:</label>
          <Select value={contextFilter} onValueChange={handleContextFilterChange}>
            <SelectTrigger className="w-[180px]">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Data</SelectItem>
              <SelectItem value="root">By Root</SelectItem>
              <SelectItem value="scan">By Scan ID</SelectItem>
            </SelectContent>
          </Select>

          {contextFilter !== 'all' && (
            <Input
              type="text"
              value={contextValue}
              onChange={(e) => setContextValue(e.target.value)}
              placeholder={
                contextFilter === 'root'
                  ? 'Enter root ID...'
                  : 'Enter scan ID...'
              }
              className="flex-1 max-w-md"
            />
          )}
        </div>

        <TabsContent value="alerts" className="flex-1">
          <AlertsTab contextFilter={contextFilter} contextValue={contextValue} />
        </TabsContent>

        <TabsContent value="statistics" className="flex-1">
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Coming soon
          </div>
        </TabsContent>

        <TabsContent value="changes" className="flex-1">
          <div className="flex items-center justify-center h-64 text-muted-foreground">
            Coming soon
          </div>
        </TabsContent>
      </Tabs>
    </div>
  )
}
