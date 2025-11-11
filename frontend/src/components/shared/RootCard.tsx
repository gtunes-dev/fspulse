import type { ReactNode } from 'react'
import { Card, CardContent, CardHeader } from '@/components/ui/card'
import { RootPicker } from '@/components/shared/RootPicker'

interface Root {
  root_id: number
  root_path: string
}

interface RootCardProps {
  /** Available roots to choose from */
  roots: Root[]
  /** Currently selected root ID */
  selectedRootId: string
  /** Callback when root selection changes */
  onRootChange: (rootId: string) => void
  /** Optional action bar content (filters, controls, etc.) */
  actionBar?: ReactNode
  /** Main card content (will change based on selected root) */
  children: ReactNode
  /** Optional className for the card */
  className?: string
  /** Allow selecting "All Roots" as an option */
  allowAll?: boolean
}

/**
 * Root Card Pattern
 *
 * A specialized card where the RootPicker serves as the header.
 * Use this when the entire card's content is dependent on which root is selected.
 *
 * Key Don Norman principles:
 * - Visibility: Most important control (root selection) in most prominent position
 * - Mapping: Header control clearly defines what content below shows
 * - Affordance: Interactive header signals "this is the primary control"
 * - No redundancy: Root path appears once, where it matters most
 *
 * @example
 * <RootCard
 *   roots={roots}
 *   selectedRootId={selectedId}
 *   onRootChange={setSelectedId}
 *   actionBar={
 *     <>
 *       <SearchFilter ... />
 *       <Checkbox ... />
 *     </>
 *   }
 * >
 *   <FileTreeView rootId={selectedRootId} />
 * </RootCard>
 */
export function RootCard({
  roots,
  selectedRootId,
  onRootChange,
  actionBar,
  children,
  className,
  allowAll = false,
}: RootCardProps) {
  return (
    <Card className={className}>
      <CardHeader>
        <RootPicker
          roots={roots}
          value={selectedRootId}
          onChange={onRootChange}
          variant="title"
          allowAll={allowAll}
        />
      </CardHeader>
      <CardContent className="space-y-4">
        {actionBar && (
          <div className="flex items-center gap-4">
            {actionBar}
          </div>
        )}
        {children}
      </CardContent>
    </Card>
  )
}
