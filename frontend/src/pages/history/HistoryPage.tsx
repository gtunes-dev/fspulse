import { ScanHistoryTable } from './ScanHistoryTable'
import { TaskHistoryTable } from './TaskHistoryTable'

export function HistoryPage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">History</h1>
      <ScanHistoryTable />
      <TaskHistoryTable />
    </div>
  )
}
