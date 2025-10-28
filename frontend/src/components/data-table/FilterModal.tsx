import { useState } from 'react'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Input } from '@/components/ui/input'
import { validateFilter } from '@/lib/api'
import type { ColumnState } from '@/lib/types'

interface FilterModalProps {
  column: ColumnState | null
  domain: string
  open: boolean
  onClose: () => void
  onApply: (columnName: string, filterValue: string) => void
}

export function FilterModal({ column, domain, open, onClose, onApply }: FilterModalProps) {
  const [filterValue, setFilterValue] = useState('')
  const [error, setError] = useState<string | null>(null)
  const [validating, setValidating] = useState(false)

  const handleApply = async () => {
    if (!column) return

    // Basic validation
    if (!filterValue.trim()) {
      setError('Filter value cannot be empty')
      return
    }

    // Validate with backend
    setValidating(true)
    setError(null)

    try {
      const result = await validateFilter({
        domain,
        column: column.name,
        value: filterValue.trim(),
      })

      if (!result.valid) {
        setError(result.error || 'Invalid filter value')
        setValidating(false)
        return
      }

      // Filter is valid, apply it
      onApply(column.name, filterValue.trim())
      setFilterValue('')
      setError(null)
      onClose()
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Validation failed')
    } finally {
      setValidating(false)
    }
  }

  const handleClose = () => {
    setFilterValue('')
    setError(null)
    onClose()
  }

  if (!column) return null

  return (
    <Dialog open={open} onOpenChange={(isOpen) => !isOpen && handleClose()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Filter: {column.display_name}</DialogTitle>
          <DialogDescription>
            Enter a filter value for this column.
            {column.filter_info?.syntax_hint && (
              <div className="mt-2 text-sm">
                <strong>Syntax:</strong> {column.filter_info.syntax_hint}
              </div>
            )}
          </DialogDescription>
        </DialogHeader>

        <div className="py-4">
          <Input
            value={filterValue}
            onChange={(e) => {
              setFilterValue(e.target.value)
              setError(null)
            }}
            onKeyDown={(e) => {
              if (e.key === 'Enter') handleApply()
              if (e.key === 'Escape') handleClose()
            }}
            placeholder={`Filter ${column.display_name}...`}
            autoFocus
          />
          {error && (
            <pre className="text-sm text-destructive mt-2 whitespace-pre-wrap font-mono bg-destructive/10 p-2 rounded border border-destructive/20">
              {error}
            </pre>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={handleClose} disabled={validating}>
            Cancel
          </Button>
          <Button onClick={handleApply} disabled={validating}>
            {validating ? 'Validating...' : 'Apply Filter'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
