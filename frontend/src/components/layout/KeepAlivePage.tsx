import { useState, type ReactNode } from 'react'

interface KeepAlivePageProps {
  isActive: boolean
  children: (isActive: boolean) => ReactNode
}

/**
 * Keeps a page mounted after its first visit, toggling visibility with CSS.
 *
 * - Lazy mount: children are not rendered until isActive is true for the first time.
 * - Once mounted, children stay mounted forever (hidden via display:none when inactive).
 * - Passes isActive to children via render prop so they can disengage virtualizers,
 *   skip fetches, etc.
 *
 * Usage:
 *   <KeepAlivePage isActive={pathname === '/browse'}>
 *     {(active) => <BrowsePage isActive={active} />}
 *   </KeepAlivePage>
 */
export function KeepAlivePage({ isActive, children }: KeepAlivePageProps) {
  const [visited, setVisited] = useState(false)

  // Set visited on first activation (setting state during render is safe
  // here because the guard prevents infinite loops — recognized React pattern)
  if (isActive && !visited) {
    setVisited(true)
  }

  if (!visited) return null

  return (
    <div className={isActive ? 'flex-1 min-h-0 flex flex-col' : 'hidden'}>
      {children(isActive)}
    </div>
  )
}
