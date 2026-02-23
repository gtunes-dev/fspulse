import { useState, useEffect, useRef } from 'react'
import { useVirtualizer } from '@tanstack/react-virtual'
import { Folder, File, FileSymlink, FileQuestion, Trash2, Home } from 'lucide-react'
import { cn } from '@/lib/utils'
import { formatFileSizeCompact } from '@/lib/formatUtils'
import { formatDateRelative } from '@/lib/dateUtils'

interface FolderViewProps {
  rootId: number
  rootPath: string
  scanId: number
  showDeleted: boolean
  isActive?: boolean
  selectedItemId?: number | null
  onItemSelect?: (item: SelectedFolderItem) => void
}

export interface SelectedFolderItem {
  itemId: number
  itemPath: string
  itemType: 'F' | 'D' | 'S' | 'O'
  isTombstone: boolean
}

interface FolderItem {
  item_id: number
  item_path: string
  item_name: string
  item_type: 'F' | 'D' | 'S' | 'O'
  is_deleted: boolean
  size: number | null
  mod_date: number | null
}

type SortColumn = 'name' | 'size' | 'mod_date'
type SortDir = 'asc' | 'desc'

function sortItems(items: FolderItem[], column: SortColumn, dir: SortDir): FolderItem[] {
  return [...items].sort((a, b) => {
    // Directories always first
    if (a.item_type === 'D' && b.item_type !== 'D') return -1
    if (a.item_type !== 'D' && b.item_type === 'D') return 1

    let cmp = 0
    switch (column) {
      case 'name':
        cmp = a.item_name.localeCompare(b.item_name, undefined, { sensitivity: 'base' })
        break
      case 'size':
        cmp = (a.size ?? -1) - (b.size ?? -1)
        break
      case 'mod_date':
        cmp = (a.mod_date ?? 0) - (b.mod_date ?? 0)
        break
    }

    return dir === 'asc' ? cmp : -cmp
  })
}

function getItemIcon(type: string, deleted: boolean) {
  const cls = 'h-4 w-4'
  if (type === 'D') return <Folder className={cn(cls, deleted ? 'text-muted-foreground' : 'text-blue-500')} />
  if (type === 'S') return <FileSymlink className={cn(cls, 'text-muted-foreground')} />
  if (type === 'O') return <FileQuestion className={cn(cls, 'text-muted-foreground')} />
  return <File className={cn(cls, 'text-muted-foreground')} />
}

export function FolderView({
  rootId,
  rootPath,
  scanId,
  showDeleted,
  isActive = true,
  selectedItemId,
  onItemSelect,
}: FolderViewProps) {
  const [currentPath, setCurrentPath] = useState(rootPath)
  const [items, setItems] = useState<FolderItem[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [sortColumn, setSortColumn] = useState<SortColumn>('name')
  const [sortDir, setSortDir] = useState<SortDir>('asc')

  const parentRef = useRef<HTMLDivElement>(null)
  const requestIdRef = useRef(0)

  // Reset to root when rootPath changes
  useEffect(() => {
    setCurrentPath(rootPath)
  }, [rootPath])

  // Fetch items for currentPath (skip when not active)
  const lastFetchKeyRef = useRef<string | null>(null)
  useEffect(() => {
    if (!isActive) return

    const fetchKey = `${rootId}:${currentPath}:${scanId}`
    if (lastFetchKeyRef.current === fetchKey) return
    lastFetchKeyRef.current = fetchKey

    const currentRequestId = ++requestIdRef.current

    async function loadItems() {
      setLoading(true)
      setError(null)

      try {
        const params = new URLSearchParams({
          root_id: rootId.toString(),
          parent_path: currentPath,
          scan_id: scanId.toString(),
        })

        const response = await fetch(`/api/items/immediate-children?${params}`)
        if (currentRequestId !== requestIdRef.current) return

        if (!response.ok) {
          throw new Error(`Failed to fetch folder contents: ${response.statusText}`)
        }

        const data = (await response.json()) as Array<{
          item_id: number
          item_path: string
          item_name: string
          item_type: string
          is_deleted: boolean
          size: number | null
          mod_date: number | null
        }>

        setItems(
          data.map((item) => ({
            ...item,
            item_type: item.item_type as 'F' | 'D' | 'S' | 'O',
          }))
        )
      } catch (err) {
        if (currentRequestId !== requestIdRef.current) return
        lastFetchKeyRef.current = null // Allow retry on error
        setError(err instanceof Error ? err.message : 'Failed to load folder')
        console.error('FolderView load error:', err)
      } finally {
        if (currentRequestId === requestIdRef.current) {
          setLoading(false)
        }
      }
    }

    loadItems()
  }, [isActive, rootId, currentPath, scanId])

  // Filter and sort
  const visibleItems = showDeleted ? items : items.filter((i) => !i.is_deleted)
  const sortedItems = sortItems(visibleItems, sortColumn, sortDir)

  const virtualizer = useVirtualizer({
    count: sortedItems.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 36,
    overscan: 5,
  })

  // Build breadcrumbs from currentPath relative to rootPath
  const buildBreadcrumbs = () => {
    const crumbs: { label: string; path: string }[] = []

    // Root crumb
    const rootName = rootPath.split('/').filter(Boolean).pop() ?? '/'
    crumbs.push({ label: rootName, path: rootPath })

    if (currentPath !== rootPath) {
      const rootPrefix = rootPath.endsWith('/') ? rootPath : rootPath + '/'
      const relative = currentPath.startsWith(rootPrefix)
        ? currentPath.substring(rootPrefix.length)
        : currentPath.substring(rootPath.length)

      const segments = relative.split('/').filter(Boolean)
      let buildPath = rootPath
      for (const seg of segments) {
        buildPath = buildPath.endsWith('/') ? buildPath + seg : buildPath + '/' + seg
        crumbs.push({ label: seg, path: buildPath })
      }
    }

    return crumbs
  }

  const breadcrumbs = buildBreadcrumbs()

  const handleSort = (column: SortColumn) => {
    if (sortColumn === column) {
      setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'))
    } else {
      setSortColumn(column)
      setSortDir(column === 'name' ? 'asc' : 'desc')
    }
  }

  const handleNavigate = (path: string) => {
    setCurrentPath(path)
  }

  const handleItemSelect = (item: FolderItem) => {
    onItemSelect?.({
      itemId: item.item_id,
      itemPath: item.item_path,
      itemType: item.item_type,
      isTombstone: item.is_deleted,
    })
  }

  const sortIndicator = (column: SortColumn) => {
    if (sortColumn !== column) return null
    return <span className="ml-0.5">{sortDir === 'asc' ? '\u25B2' : '\u25BC'}</span>
  }

  return (
    <div className="flex flex-col h-full">
      {/* Breadcrumb ribbon */}
      <div className="flex items-center px-3 py-2 border-b border-border min-h-[40px]">
        <nav className="breadcrumb-ribbon">
          {breadcrumbs.map((crumb, i) => (
            <button
              key={crumb.path}
              className="breadcrumb-segment"
              onClick={() => handleNavigate(crumb.path)}
            >
              {i === 0 && <Home className="h-3.5 w-3.5" />}
              {crumb.label}
            </button>
          ))}
        </nav>
      </div>

      {/* Column headers */}
      <div className="flex items-center gap-2 px-3 py-1.5 border-b border-border text-xs font-medium text-muted-foreground uppercase tracking-wide bg-muted">
        <div className="w-7" /> {/* icon space */}
        <button
          className="flex-1 text-left hover:text-foreground transition-colors"
          onClick={() => handleSort('name')}
        >
          Name{sortIndicator('name')}
        </button>
        <button
          className="w-24 text-right hover:text-foreground transition-colors"
          onClick={() => handleSort('size')}
        >
          Size{sortIndicator('size')}
        </button>
        <button
          className="w-28 text-right hover:text-foreground transition-colors"
          onClick={() => handleSort('mod_date')}
        >
          Modified{sortIndicator('mod_date')}
        </button>
      </div>

      {/* Content */}
      {loading ? (
        <div className="flex-1 flex items-center justify-center text-muted-foreground">
          Loading...
        </div>
      ) : error ? (
        <div className="flex-1 flex items-center justify-center text-red-600">
          {error}
        </div>
      ) : sortedItems.length === 0 ? (
        <div className="flex-1 flex items-center justify-center text-muted-foreground">
          Empty folder
        </div>
      ) : (
        <div
          ref={parentRef}
          className="flex-1 min-h-0 overflow-auto"
        >
          <div
            style={{
              height: `${virtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {virtualizer.getVirtualItems().map((virtualItem) => {
              const item = sortedItems[virtualItem.index]
              const isSelected = selectedItemId === item.item_id
              const isDir = item.item_type === 'D'

              return (
                <div
                  key={item.item_id}
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    height: `${virtualItem.size}px`,
                    transform: `translateY(${virtualItem.start}px)`,
                  }}
                >
                  <div
                    className={cn(
                      'flex items-center gap-2 px-3 py-1.5 hover:bg-accent transition-colors rounded-sm',
                      isSelected && 'bg-accent',
                      item.is_deleted && 'text-muted-foreground'
                    )}
                  >
                    {/* Icon — click navigates into folders */}
                    <div className="w-7 flex items-center justify-center shrink-0">
                      {isDir && !item.is_deleted ? (
                        <button
                          className="p-0 border-none bg-transparent cursor-pointer"
                          onClick={() => handleNavigate(item.item_path)}
                          aria-label={`Open ${item.item_name}`}
                        >
                          {getItemIcon(item.item_type, item.is_deleted)}
                        </button>
                      ) : (
                        getItemIcon(item.item_type, item.is_deleted)
                      )}
                    </div>

                    {/* Name — click selects */}
                    <div
                      className={cn('flex-1 text-sm truncate cursor-pointer', item.is_deleted && 'line-through')}
                      onClick={() => handleItemSelect(item)}
                    >
                      {item.item_name}
                      {item.is_deleted && <Trash2 className="inline h-3 w-3 ml-1.5 text-muted-foreground" />}
                    </div>

                    {/* Size */}
                    <div className="w-24 text-right text-xs text-muted-foreground tabular-nums">
                      {formatFileSizeCompact(item.size)}
                    </div>

                    {/* Modified */}
                    <div className="w-28 text-right text-xs text-muted-foreground">
                      {item.mod_date ? formatDateRelative(item.mod_date) : '\u2014'}
                    </div>
                  </div>
                </div>
              )
            })}
          </div>
        </div>
      )}
    </div>
  )
}
