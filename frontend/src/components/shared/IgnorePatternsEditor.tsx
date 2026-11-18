import { Plus } from 'lucide-react'
import { Textarea } from '@/components/ui/textarea'
import { IGNORE_SUGGESTIONS, parsePatterns } from '@/lib/observation'

interface IgnorePatternsEditorProps {
  /** One pattern per line. */
  value: string
  onChange: (value: string) => void
  id?: string
}

/**
 * Edits a root's ignore patterns as text, one per line, and offers
 * common ones to add with a click. Suggestions are only offered; none
 * is in effect until it is in the list.
 */
export function IgnorePatternsEditor({ value, onChange, id }: IgnorePatternsEditorProps) {
  const current = new Set(parsePatterns(value))
  const offered = IGNORE_SUGGESTIONS.filter((suggestion) => !current.has(suggestion.pattern))

  const add = (pattern: string) => {
    const trimmed = value.trimEnd()
    onChange(trimmed ? `${trimmed}\n${pattern}` : pattern)
  }

  return (
    <div className="space-y-2">
      <Textarea
        id={id}
        value={value}
        onChange={(event) => onChange(event.target.value)}
        placeholder="One pattern per line, for example node_modules"
        className="font-mono text-sm min-h-24"
        spellCheck={false}
      />

      {offered.length > 0 && (
        <div className="space-y-1">
          <p className="text-xs text-muted-foreground">Suggestions (click to add):</p>
          <div className="flex flex-wrap gap-1.5">
            {offered.map((suggestion) => (
              <button
                key={suggestion.pattern}
                type="button"
                onClick={() => add(suggestion.pattern)}
                title={suggestion.description}
                className="inline-flex items-center gap-1 rounded-md border border-border px-2 py-0.5 text-xs font-mono hover:bg-accent transition-colors cursor-pointer"
              >
                <Plus className="h-3 w-3" />
                {suggestion.pattern}
              </button>
            ))}
          </div>
        </div>
      )}

      <p className="text-xs text-muted-foreground">
        A name such as <span className="font-mono">node_modules</span> or{' '}
        <span className="font-mono">*.tmp</span> matches at any depth. A path such as{' '}
        <span className="font-mono">Library/Caches</span> is matched from the top of the root.
        Ignoring a folder ignores everything in it.
      </p>
    </div>
  )
}
