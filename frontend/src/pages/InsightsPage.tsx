import { ScanTrendsTab } from './insights/ScanTrendsTab'

export function InsightsPage() {
  return (
    <div className="flex flex-col h-full">
      <h1 className="text-2xl font-semibold mb-6">Insights</h1>
      <ScanTrendsTab />
    </div>
  )
}
