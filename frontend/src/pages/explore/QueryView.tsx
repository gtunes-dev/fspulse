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

const ITEMS_PER_PAGE = 25

const SAMPLE_QUERIES = [
  {
    label: 'Basic',
    query: 'items limit 10',
  },
  {
    label: 'Filter files',
    query: 'items where item_type:(F) show item_path, size limit 25',
  },
  {
    label: 'Large files',
    query: 'items where item_type:(F), size:(>1000000) show item_path, size order by size desc limit 20',
  },
  {
    label: 'Open alerts',
    query: 'alerts where alert_status:(O) show alert_type, item_path, created_at limit 15',
  },
  {
    label: 'Deleted versions',
    query: 'versions where is_deleted:(T) show item_path, item_type, first_scan_id, last_scan_id order by last_scan_id desc limit 20',
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
  const [totalCount, setTotalCount] = useState(0)
  const [currentPage, setCurrentPage] = useState(1)
  const [executedQuery, setExecutedQuery] = useState('')

  const fetchPage = async (queryStr: string, page: number) => {
    const response = await fetch('/api/query/fetch_override', {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        query: queryStr,
        limit_override: ITEMS_PER_PAGE,
        offset_add: (page - 1) * ITEMS_PER_PAGE,
      }),
    })

    if (!response.ok) {
      const errorText = await response.text()
      throw new Error(errorText || 'Query execution failed')
    }

    return response.json()
  }

  const handleExecuteQuery = async () => {
    if (!query.trim()) {
      setError('Query cannot be empty')
      return
    }

    setLoading(true)
    setError(null)

    try {
      // Get count first
      const countResponse = await fetch('/api/query/count_raw', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
        },
        body: JSON.stringify({ query: query.trim() }),
      })

      if (!countResponse.ok) {
        const errorText = await countResponse.text()
        throw new Error(errorText || 'Count query failed')
      }

      const countData = await countResponse.json()
      setTotalCount(countData.count)
      setExecutedQuery(query.trim())
      setCurrentPage(1)

      // Fetch first page
      const data = await fetchPage(query.trim(), 1)
      setResult(data)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to execute query')
      setResult(null)
      setTotalCount(0)
    } finally {
      setLoading(false)
    }
  }

  const handlePageChange = async (newPage: number) => {
    if (!executedQuery) return

    setLoading(true)
    setError(null)

    try {
      const data = await fetchPage(executedQuery, newPage)
      setResult(data)
      setCurrentPage(newPage)
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to fetch page')
    } finally {
      setLoading(false)
    }
  }

  const handleSampleClick = (sampleQuery: string) => {
    setQuery(sampleQuery)
    setError(null)
    setResult(null)
    setTotalCount(0)
    setCurrentPage(1)
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
            placeholder="Enter your FsPulse query here... (e.g., items where item_type:(F) show item_path, size limit 10)"
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
            <div className="flex flex-col h-full">
              {result.rows.length > 0 ? (
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
              ) : (
                <div className="p-8 text-center text-muted-foreground">
                  No results found
                </div>
              )}

              {/* Pagination */}
              {totalCount > 0 && (
                <div className="flex items-center justify-between p-4 border-t border-border">
                  <div className="text-sm text-muted-foreground">
                    Showing {((currentPage - 1) * ITEMS_PER_PAGE + 1).toLocaleString()} to{' '}
                    {Math.min(currentPage * ITEMS_PER_PAGE, totalCount).toLocaleString()} of {totalCount.toLocaleString()}
                  </div>
                  <div className="flex items-center gap-2">
                    <button
                      onClick={() => currentPage > 1 && handlePageChange(currentPage - 1)}
                      disabled={currentPage === 1 || loading}
                      className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
                    >
                      Previous
                    </button>
                    <button
                      onClick={() => currentPage * ITEMS_PER_PAGE < totalCount && handlePageChange(currentPage + 1)}
                      disabled={currentPage * ITEMS_PER_PAGE >= totalCount || loading}
                      className="px-3 py-1.5 border border-border rounded-md text-sm disabled:opacity-50 disabled:cursor-not-allowed hover:bg-accent hover:text-accent-foreground transition-colors"
                    >
                      Next
                    </button>
                  </div>
                </div>
              )}
            </div>
          </CardContent>
        </Card>
      )}
    </div>
  )
}
