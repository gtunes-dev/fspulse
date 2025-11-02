import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

interface Root {
  root_id: number
  root_path: string
}

interface RootPickerProps {
  roots: Root[]
  value: string
  onChange: (value: string) => void
  placeholder?: string
}

export function RootPicker({ roots, value, onChange, placeholder = 'Select a root' }: RootPickerProps) {
  const selectedRoot = roots.find(r => r.root_id.toString() === value)

  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className="h-12 w-[380px] font-medium shadow-sm ring-1 ring-border/50 hover:ring-border transition-all">
        <div className="flex items-center gap-2.5 w-full">
          <span className="text-sm font-semibold text-muted-foreground/80">Root:</span>
          <SelectValue className="flex-1">
            {selectedRoot ? selectedRoot.root_path : placeholder}
          </SelectValue>
        </div>
      </SelectTrigger>
      <SelectContent>
        {roots.map((root) => (
          <SelectItem key={root.root_id} value={root.root_id.toString()}>
            {root.root_path}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
