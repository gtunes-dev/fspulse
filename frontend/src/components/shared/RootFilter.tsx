import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

interface Root {
  id: number
  path: string
}

interface RootFilterProps {
  roots: Root[]
  selectedRootId: string
  onRootChange: (value: string) => void
  label?: string
}

/**
 * Shared component for filtering by root
 * Used in SchedulesTable, ScanHistoryTable, etc.
 */
export function RootFilter({
  roots,
  selectedRootId,
  onRootChange,
  label = 'Filter by root:',
}: RootFilterProps) {
  if (roots.length === 0) {
    return null
  }

  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <Select value={selectedRootId} onValueChange={onRootChange}>
        <SelectTrigger className="w-[300px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">All Roots</SelectItem>
          {roots.map(root => (
            <SelectItem key={root.id} value={root.id.toString()}>
              {root.path}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  )
}
