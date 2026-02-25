import { createContext, useContext, useLayoutEffect, useState } from 'react'
import type { RefObject } from 'react'

/**
 * Provides a reference to the <main> scroll container element.
 * Used by virtualized list views to scroll at the page level
 * instead of using their own internal scroll containers.
 */
const ScrollContext = createContext<HTMLElement | null>(null)

export const ScrollProvider = ScrollContext.Provider

export function useScrollElement(): HTMLElement | null {
  return useContext(ScrollContext)
}

/**
 * Measures the distance from a container element to the <main> scroll element.
 * Used by virtualized lists that scroll via <main> to tell TanStack Virtual
 * how far the list is offset from the scroll container's top.
 */
export function useScrollMargin(parentRef: RefObject<HTMLElement | null>): number {
  const scrollElement = useScrollElement()
  const [scrollMargin, setScrollMargin] = useState(0)

  useLayoutEffect(() => {
    const parentEl = parentRef.current
    if (!parentEl || !scrollElement) return

    const measure = () => {
      const parentRect = parentEl.getBoundingClientRect()
      const scrollRect = scrollElement.getBoundingClientRect()
      setScrollMargin(parentRect.top - scrollRect.top + scrollElement.scrollTop)
    }

    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(parentEl)
    return () => observer.disconnect()
  }, [parentRef, scrollElement])

  return scrollMargin
}
