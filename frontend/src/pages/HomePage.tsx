import { QuickActionCard } from '@/components/home/QuickActionCard'
import { LastScanCard } from '@/components/home/LastScanCard'
import {
  FolderSearch,
  Lightbulb,
  Database,
  BookOpen,
} from 'lucide-react'

export function HomePage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Home</h1>

      {/* Quick Actions Grid */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
        <QuickActionCard
          icon={<FolderSearch className="w-full h-full" />}
          title="Scan"
          description="Start and manage scans of your filesystem roots"
          to="/scan"
        />

        <QuickActionCard
          icon={<Lightbulb className="w-full h-full" />}
          title="Insights"
          description="View alerts, statistics, and insights about your filesystem and scans"
          to="/insights"
        />

        <QuickActionCard
          icon={<Database className="w-full h-full" />}
          title="Explore"
          description="Interactively explore your data and execute custom queries"
          to="/explore"
        />

        <QuickActionCard
          icon={<BookOpen className="w-full h-full" />}
          title="Documentation"
          description="Read the user guide and API reference documentation"
          href="https://gtunes-dev.github.io/fspulse/"
        />
      </div>

      {/* Last Scan Card */}
      <LastScanCard />
    </div>
  )
}
