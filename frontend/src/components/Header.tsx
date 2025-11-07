import { Moon, Sun } from 'lucide-react'
import { useTheme } from '@/hooks/useTheme'
import { useScanManager } from '@/contexts/ScanManagerContext'
import { useNavigate } from 'react-router-dom'

function shortenPath(path: string, maxLength = 50): string {
  if (!path || path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return '.../' + parts.slice(-2).join('/')
}

export function Header() {
  const { theme, toggleTheme } = useTheme()
  const { activeScan } = useScanManager()
  const navigate = useNavigate()

  // Get current scan data
  const currentScan = activeScan
  const isScanning = currentScan !== null

  // Compute display data for scan progress
  let scanStatus = ''
  let scanPath = ''
  let scanPhase = ''
  let scanItems = ''
  let showProgressBar = false
  let progressPercentage = 0
  let isError = false

  if (currentScan) {
    const statusValue = currentScan.status?.status || 'running'
    scanStatus = statusValue === 'running' ? 'Scanning' : statusValue === 'cancelling' ? 'Cancelling' : 'Processing'
    scanPath = shortenPath(currentScan.root_path)

    const phaseNames = ['Scanning Files', 'Tombstoning Deletes', 'Analyzing']
    scanPhase = phaseNames[currentScan.phase - 1] || 'Processing'

    // Phase-specific details
    if (statusValue === 'error' && currentScan.error_message) {
      // Error state
      isError = true
      scanStatus = ''
      scanPhase = `Error: ${currentScan.error_message}`
      scanItems = ''
      showProgressBar = false
    } else if (currentScan.phase === 3 && currentScan.progress) {
      // Analysis phase: show X/Y files and percentage with progress bar
      scanItems = `${currentScan.progress.current.toLocaleString()} / ${currentScan.progress.total.toLocaleString()} files`
      showProgressBar = true
      progressPercentage = Math.round(currentScan.progress.percentage)
    } else if (currentScan.scanning_counts) {
      // Phases 1 & 2: show file and directory counts, no progress bar
      const files = currentScan.scanning_counts.files.toLocaleString()
      const dirs = currentScan.scanning_counts.directories.toLocaleString()
      scanItems = `${files} files in ${dirs} directories`
      showProgressBar = false
    } else {
      // Fallback if no counts available yet
      scanItems = 'Scanning...'
      showProgressBar = false
    }
  }

  return (
    <header className="border-b border-border bg-card">
      <div className="flex h-16 items-center justify-between px-6">
        <div className="flex items-center gap-4 flex-1 min-w-0">
          <h1 className="text-xl font-semibold flex-shrink-0">FsPulse</h1>

          {/* Scan Progress Indicator */}
          {isScanning && currentScan && (
            <div
              className={`flex-1 ml-8 px-3 py-1 rounded-md cursor-pointer transition-colors hover:bg-primary/5 ${
                isError ? '' : 'hover:bg-primary/5'
              }`}
              onClick={() => navigate('/scan')}
            >
              <div className="flex flex-col gap-0.5">
                {/* Line 1: Status and Path */}
                <div className="text-sm font-medium leading-none overflow-hidden text-overflow-ellipsis whitespace-nowrap">
                  {scanStatus}{scanStatus && ':'} {scanPath}
                </div>

                {/* Line 2: Phase, Items, Progress Bar, Percentage */}
                <div className="flex items-center gap-2 text-xs text-muted-foreground leading-none">
                  <span className={isError ? 'text-red-600 dark:text-red-400 font-mono' : ''}>
                    {scanPhase}
                  </span>

                  {scanItems && (
                    <>
                      <span>•</span>
                      <span>{scanItems}</span>
                    </>
                  )}

                  {showProgressBar && (
                    <>
                      <span>•</span>
                      <div className="flex-1 max-w-[200px] h-[3px] bg-muted rounded-sm overflow-hidden">
                        <div
                          className="h-full bg-primary transition-all duration-300 rounded-sm"
                          style={{ width: `${progressPercentage}%` }}
                        />
                      </div>
                      <span className="min-w-[35px] text-right font-medium text-foreground">
                        {progressPercentage}%
                      </span>
                    </>
                  )}
                </div>
              </div>
            </div>
          )}
        </div>

        <div className="flex items-center gap-4 flex-shrink-0">
          <button
            onClick={toggleTheme}
            className="rounded-md p-2 hover:bg-muted transition-colors"
            title={`Switch to ${theme === 'light' ? 'dark' : 'light'} mode`}
          >
            {theme === 'light' ? (
              <Moon className="h-5 w-5" />
            ) : (
              <Sun className="h-5 w-5" />
            )}
          </button>
        </div>
      </div>
    </header>
  )
}
