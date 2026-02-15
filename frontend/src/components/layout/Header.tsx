import { Moon, Sun } from 'lucide-react'
import { useTheme } from '@/hooks/useTheme'
import { useTaskContext } from '@/contexts/TaskContext'
import { useNavigate } from 'react-router-dom'

function shortenPath(path: string, maxLength = 50): string {
  if (!path || path.length <= maxLength) return path
  const parts = path.split('/')
  if (parts.length <= 2) return path
  return '.../' + parts.slice(-2).join('/')
}

export function Header() {
  const { theme, toggleTheme } = useTheme()
  const { activeTask } = useTaskContext()
  const navigate = useNavigate()

  const isRunning = activeTask !== null
  const isError = activeTask?.status === 'error'

  // Derive display values from activeTask
  const headerText = activeTask
    ? (activeTask.target ? `${activeTask.action}: ${shortenPath(activeTask.target)}` : activeTask.action)
    : ''

  const phaseText = activeTask?.phase ?? ''
  const progressMessage = activeTask?.progress_bar?.message ?? ''
  const hasPercentage = activeTask?.progress_bar?.percentage !== null && activeTask?.progress_bar?.percentage !== undefined
  const percentage = hasPercentage ? Math.round(activeTask!.progress_bar!.percentage!) : 0

  return (
    <header className="border-b border-border bg-card">
      <div className="flex h-16 items-center justify-between px-6">
        <div className="flex items-center gap-4 flex-1 min-w-0">
          <h1 className="text-xl font-semibold flex-shrink-0">FsPulse</h1>

          {/* Task Progress Indicator */}
          {isRunning && activeTask && (
            <div
              className={`flex-1 ml-8 px-3 py-1 rounded-md cursor-pointer transition-colors ${
                isError ? '' : 'hover:bg-primary/5'
              }`}
              onClick={() => navigate('/')}
            >
              <div className="flex flex-col gap-0.5">
                {/* Line 1: Action and Target */}
                <div className="text-sm font-medium leading-none overflow-hidden text-overflow-ellipsis whitespace-nowrap">
                  {headerText}
                </div>

                {/* Line 2: Phase, Progress Message, Progress Bar, Percentage */}
                <div className="flex items-center gap-2 text-xs text-muted-foreground leading-none">
                  {isError && activeTask.error_message ? (
                    <span className="text-red-600 dark:text-red-400 font-mono">
                      Error: {activeTask.error_message}
                    </span>
                  ) : (
                    <>
                      {phaseText && <span>{phaseText}</span>}

                      {progressMessage && (
                        <>
                          {phaseText && <span>•</span>}
                          <span>{progressMessage}</span>
                        </>
                      )}

                      {hasPercentage && (
                        <>
                          <span>•</span>
                          <div className="flex-1 max-w-[200px] h-[3px] bg-muted rounded-sm overflow-hidden">
                            <div
                              className="h-full bg-primary transition-all duration-300 rounded-sm"
                              style={{ width: `${percentage}%` }}
                            />
                          </div>
                          <span className="min-w-[35px] text-right font-medium text-foreground">
                            {percentage}%
                          </span>
                        </>
                      )}
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
