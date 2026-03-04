import { Card, CardContent } from '@/components/ui/card'

export function HistoryPage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">History</h1>
      <Card>
        <CardContent className="pt-6">
          <p className="text-sm text-muted-foreground text-center py-8">
            Scan and task history will be available here.
          </p>
        </CardContent>
      </Card>
    </div>
  )
}
