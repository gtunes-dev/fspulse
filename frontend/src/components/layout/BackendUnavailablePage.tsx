import { Loader2 } from 'lucide-react'

export function BackendUnavailablePage() {
  return (
    <div className="flex h-screen items-center justify-center bg-background">
      <div className="flex flex-col items-center gap-4 text-center">
        <Loader2 className="h-8 w-8 animate-spin text-muted-foreground" />
        <div>
          <h2 className="text-lg font-semibold">Server Unavailable</h2>
          <p className="text-sm text-muted-foreground mt-1">
            Waiting for the FsPulse server...
          </p>
        </div>
      </div>
    </div>
  )
}
