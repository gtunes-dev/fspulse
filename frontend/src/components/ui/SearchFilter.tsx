import { Search } from 'lucide-react'
import { Input } from '@/components/ui/input'

interface SearchFilterProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
  className?: string
}

export function SearchFilter({ value, onChange, placeholder = 'Search', className }: SearchFilterProps) {
  return (
    <div className={`relative flex-1 max-w-md ${className || ''}`}>
      <Search className="absolute left-3.5 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground/60" />
      <Input
        type="text"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="h-10 pl-10 shadow-sm ring-1 ring-border/50 hover:ring-border transition-all"
      />
    </div>
  )
}
