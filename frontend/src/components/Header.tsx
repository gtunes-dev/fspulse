import { Moon, Sun } from 'lucide-react'
import { useTheme } from '@/hooks/useTheme'

export function Header() {
  const { theme, toggleTheme } = useTheme()

  return (
    <header className="border-b border-border bg-card">
      <div className="flex h-16 items-center justify-between px-6">
        <div className="flex items-center gap-4">
          <h1 className="text-xl font-semibold">FsPulse</h1>
        </div>

        <div className="flex items-center gap-4">
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
