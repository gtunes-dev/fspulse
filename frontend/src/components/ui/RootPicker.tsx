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
  /** Visual variant - 'default' for form-like appearance, 'title' for CardTitle appearance */
  variant?: 'default' | 'title'
}

export function RootPicker({
  roots,
  value,
  onChange,
  placeholder = 'Select a root',
  variant = 'default'
}: RootPickerProps) {
  const selectedRoot = roots.find(r => r.root_id.toString() === value)

  // Title variant: looks like CardTitle (text-2xl font-semibold, no border)
  // Everything flows as one cohesive title with subtle dropdown affordance
  const triggerClassName = variant === 'title'
    ? "h-auto w-fit border-none shadow-none p-0 gap-1.5 focus:ring-0 focus:ring-offset-0 hover:bg-transparent [&>svg]:h-5 [&>svg]:w-5 [&>svg]:shrink-0"
    : "h-12 w-[380px] font-medium shadow-sm ring-1 ring-border/50 hover:ring-border transition-all"

  const contentClassName = variant === 'title'
    ? "text-2xl font-semibold leading-none tracking-tight flex items-baseline gap-1.5"
    : "flex items-center gap-2.5 w-full"

  const labelClassName = variant === 'title'
    ? "text-muted-foreground"
    : "text-sm font-semibold text-muted-foreground/80"

  return (
    <Select value={value} onValueChange={onChange}>
      <SelectTrigger className={triggerClassName}>
        <div className={contentClassName}>
          <span className={labelClassName}>Root:</span>
          <SelectValue>
            {selectedRoot ? selectedRoot.root_path : placeholder}
          </SelectValue>
        </div>
      </SelectTrigger>
      <SelectContent align="start" className="max-w-2xl">
        {roots.map((root) => (
          <SelectItem key={root.root_id} value={root.root_id.toString()}>
            {root.root_path}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
