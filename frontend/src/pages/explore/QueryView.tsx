import { useState } from 'react'
import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card'
import { Button } from '@/components/ui/button'
import { Textarea } from '@/components/ui/textarea'
import {
  Accordion,
  AccordionContent,
  AccordionItem,
  AccordionTrigger,
} from '@/components/ui/accordion'
import { BookOpenText, ExternalLink } from 'lucide-react'
import type { Alignment } from '@/lib/types'

interface QueryResult {
  columns: string[]
  rows: string[][]
  alignments: Alignment[]
}

const SAMPLE_QUERIES = [
  {
    label: 'Basic',
    query: 'items limit 10',
  },
  {
    label: 'Filter files',
    query: 'items where item_type:(F) show item_path, file_size limit 25',
  },
  {
    label: 'Large files',
    query: 'items where file_size:(>1000000) show item_path, file_size order by file_size desc limit 20',
  },
  {
    label: 'Open alerts',
    query: 'alerts where alert_status:(O) show alert_type, item_path, created_at limit 15',
  },
  {
    label: 'Changed to Invalid',
    query: 'changes where val_change:(T), val_old:(I, N, U), val_new:(I) show root_id, scan_id, item_id, item_path, val_error_new order by item_path desc limit 20',
  },
]

const getAlignmentClass = (alignment: Alignment): string => {
  switch (alignment) {
    case 'Left':
      return 'text-left'
    case 'Center':
      return 'text-center'
    case 'Right':
      return 'text-right'
    default:
      return 'text-left'
  }
}

export function QueryView() {
  const [query, setQuery] = useState('')
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [result, setResult] = useState<QueryResult | null>(null)

  const handleExecuteQuery = async () => {
    if (!query.trim()) {
      setError('Query cannot be empty')
      return
    }

    setLoading(true)
    setError(null)

    try {
      const response = await fetch('/api/query/execute', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ query: query.trim() }),
      })

      if (!response.ok) {
        const errorText = await response.text()
        throw new Error(errorText || 'Query execution failed')
      }

      const data = await response.json()
      setResult(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to execute query')
      setResult(null)
    } finally {
      setLoading(false)
    }
  }

  const handleSampleClick = (sampleQuery: string) => {
    setQuery(sampleQuery)
    setError(null)
    setResult(null)
  }

  return (
    <div className="flex flex-col gap-6">
      {/* Query Input Card */}
      <Card>
        <CardHeader>
          <CardTitle>Execute FsPulse Query</CardTitle>
        </CardHeader>
        <CardContent className="space-y-4">
          {/* Query Textarea */}
          <Textarea
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Enter your FsPulse query here... (e.g., items where item_type:(F) show item_path, file_size limit 10)"
            className="min-h-[120px] font-mono text-sm resize-y"
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
                handleExecuteQuery()
              }
            }}
          />

          {/* Action Buttons and Documentation */}
          <div className="flex justify-between items-center">
            <div className="flex gap-4 items-center">
              <Button onClick={handleExecuteQuery} disabled={loading}>
                {loading ? 'Executing...' : 'Execute Query'}
              </Button>
              {loading && (
                <span className="text-sm text-muted-foreground">Running query...</span>
              )}
            </div>
            <a
              href="https://gtunes-dev.github.io/fspulse/query.html"
              target="_blank"
              rel="noopener noreferrer"
              className="flex items-center gap-2 text-sm text-primary hover:underline"
            >
              <BookOpenText className="h-4 w-4" />
              <span>Query Documentation</span>
              <ExternalLink className="h-3 w-3" />
            </a>
          </div>

          {/* Error Display */}
          {error && (
            <div className="bg-destructive/10 border border-destructive/20 rounded-md p-4">
              <pre className="text-sm text-destructive whitespace-pre-wrap font-mono overflow-x-auto">
                {error}
              </pre>
            </div>
          )}

          {/* Example Queries Accordion */}
          <Accordion type="single" collapsible className="bg-muted rounded-md">
            <AccordionItem value="examples" className="border-0">
              <AccordionTrigger className="px-4 py-3 hover:no-underline">
                <span className="text-sm font-medium">Example Queries</span>
              </AccordionTrigger>
              <AccordionContent className="px-4 pb-4">
                <div className="space-y-2 text-sm font-mono">
                  {SAMPLE_QUERIES.map((sample, index) => (
                    <div
                      key={index}
                      onClick={() => handleSampleClick(sample.query)}
                      className="cursor-pointer hover:bg-background/50 p-2 rounded transition-colors"
                    >
                      <strong>{sample.label}:</strong> {sample.query}
                    </div>
                  ))}
                </div>
              </AccordionContent>
            </AccordionItem>
          </Accordion>
        </CardContent>
      </Card>

      {/* Query Results */}
      {result && (
        <Card>
          <CardContent className="p-0">
            <div className="overflow-auto">
              <table className="w-full border-collapse">
                <thead className="bg-muted sticky top-0">
                  <tr>
                    {result.columns.map((col, index) => (
                      <th
                        key={index}
                        className="border border-border px-4 py-2 font-medium text-center uppercase text-xs tracking-wide"
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {result.rows.map((row, rowIndex) => (
                    <tr key={rowIndex} className="hover:bg-muted/50">
                      {row.map((cell, cellIndex) => (
                        <td
                          key={cellIndex}
                          className={`border border-border px-4 py-2 ${getAlignmentClass(result.alignments[cellIndex])}`}
                        >
                          {cell}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
