import { ScanHistoryTable } from './ScanHistoryTable'

export function ScansPage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Scans</h1>
      <ScanHistoryTable />
    </div>
  )
}
