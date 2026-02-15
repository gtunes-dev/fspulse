import { ScanHistoryTable } from './ScanHistoryTable'

export function ScansPage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Scan History</h1>
      <ScanHistoryTable />
    </div>
  )
}
