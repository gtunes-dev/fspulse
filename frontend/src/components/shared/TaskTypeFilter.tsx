import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

interface TaskTypeFilterProps {
  selectedType: string
  onTypeChange: (value: string) => void
  label?: string
}

/**
 * Shared component for filtering by task type
 * Used in TaskHistoryTable
 */
export function TaskTypeFilter({
  selectedType,
  onTypeChange,
  label = 'Filter by type:',
}: TaskTypeFilterProps) {
  return (
    <div className="flex items-center gap-2">
      <span className="text-sm text-muted-foreground">{label}</span>
      <Select value={selectedType} onValueChange={onTypeChange}>
        <SelectTrigger className="w-[200px]">
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          <SelectItem value="all">All Types</SelectItem>
          <SelectItem value="0">Scan</SelectItem>
          <SelectItem value="1">Compact Database</SelectItem>
        </SelectContent>
      </Select>
    </div>
  )
}
