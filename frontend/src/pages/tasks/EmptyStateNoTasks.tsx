import { Card, CardContent } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { useNavigate } from 'react-router-dom'
import { ExternalLink } from 'lucide-react'

interface EmptyStateNoTasksProps {
  rootCount: number
}

export function EmptyStateNoTasks({ rootCount }: EmptyStateNoTasksProps) {
  const navigate = useNavigate()

  return (
    <div className="flex flex-col gap-6">
      <Card>
        <CardContent className="pt-6">
          <div className="flex flex-col items-center justify-center py-12 text-center">
            <h3 className="text-xl font-semibold mb-3">Ready to get started?</h3>
            <p className="text-base text-muted-foreground mb-6 max-w-md">
              You've configured {rootCount === 1 ? 'a root' : 'roots'}! Use the Manual Scan button above to start scanning, or visit Monitor to add more roots and configure schedules.
            </p>
            <Button size="lg" onClick={() => navigate('/monitor')}>
              Go to Monitor
            </Button>
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
                    onClick={() => navigate('/monitor')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Add a directory to <strong>Monitor</strong>. FsPulse calls the directories it monitors "Roots"
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
                    onClick={() => navigate('/monitor')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Use the <strong>Monitor</strong> page to immediately scan a Root or to set up a recurring schedule
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
                    onClick={() => navigate('/insights/scan-trends')}
                    className="text-base text-left hover:underline flex items-center gap-2 w-full group-hover:text-primary transition-colors"
                  >
                    <span>
                      Use <strong>Insights</strong> to see historic scan trends and key information
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
