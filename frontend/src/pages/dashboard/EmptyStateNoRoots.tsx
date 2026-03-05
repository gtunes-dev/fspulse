import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useNavigate } from 'react-router-dom'
import { ExternalLink } from 'lucide-react'

export function EmptyStateNoRoots() {
  const navigate = useNavigate()

  return (
    <div className="flex flex-col gap-6">
      <Card>
        <CardContent className="pt-6">
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <h3 className="text-xl font-semibold mb-3">Welcome to fsPulse</h3>
            <p className="text-base text-muted-foreground mb-6 max-w-md">
              Get started by adding a root directory to monitor.
            </p>
            <Button size="lg" onClick={() => navigate('/roots')}>
              Go to Roots
            </Button>
            <div className="mt-8 text-sm text-muted-foreground space-y-1">
              <p>fsPulse never modifies your files</p>
              <p>fsPulse makes no outbound requests</p>
              <p>fsPulse only scans what you tell it to</p>
            </div>
          </div>
        </CardContent>
      </Card>

      <Card>
        <CardContent className="pt-6 pb-8">
          <div className="py-4 px-6 max-w-2xl mx-auto">
            <h4 className="text-lg font-semibold mb-6">Quick Start Guide</h4>
            <ol className="space-y-4">
              <li className="flex items-start gap-4 group">
                <span className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground text-sm font-semibold mt-0.5">
                  1
                </span>
                <div className="flex-1 min-w-0">
                  <button
                    onClick={() => navigate('/roots')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Add a directory on the <strong>Roots</strong> page. fsPulse calls the directories it monitors "Roots"
                    </span>
                    <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
                  </button>
                </div>
              </li>
              <li className="flex items-start gap-4 group">
                <span className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground text-sm font-semibold mt-0.5">
                  2
                </span>
                <div className="flex-1 min-w-0">
                  <button
                    onClick={() => navigate('/schedules')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Create a recurring scan schedule on the <strong>Schedules</strong> page, or use <strong>Scan Now</strong> on any root for an immediate scan
                    </span>
                    <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
                  </button>
                </div>
              </li>
              <li className="flex items-start gap-4 group">
                <span className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground text-sm font-semibold mt-0.5">
                  3
                </span>
                <div className="flex-1 min-w-0">
                  <button
                    onClick={() => navigate('/trends/scan-trends')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Use <strong>Trends</strong> to see historic scan trends and key information
                    </span>
                    <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
                  </button>
                </div>
              </li>
              <li className="flex items-start gap-4 group">
                <span className="flex h-7 w-7 flex-shrink-0 items-center justify-center rounded-full bg-primary text-primary-foreground text-sm font-semibold mt-0.5">
                  4
                </span>
                <div className="flex-1 min-w-0">
                  <button
                    onClick={() => navigate('/alerts')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Visit <strong>Alerts</strong> to see issues that have been detected in specific files
                    </span>
                    <ExternalLink className="h-4 w-4 flex-shrink-0 text-muted-foreground" />
                  </button>
                </div>
              </li>
            </ol>
          </div>
        </CardContent>
      </Card>
    </div>
  )
}
