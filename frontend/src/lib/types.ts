// Types matching the backend API responses and structures

export type SortDirection = 'none' | 'asc' | 'desc'

export type Alignment = 'Left' | 'Right' | 'Center'

export type ColumnType =
  | 'Id'
  | 'Path'
  | 'String'
  | 'Date'
  | 'Bool'
  | 'Int'
  | 'ItemType'
  | 'ChangeType'
  | 'AlertType'
  | 'AlertStatus'
  | 'Val'

export interface FilterInfo {
  type_name: string
  syntax_hint: string
}

export interface ColumnMetadata {
  name: string
  display_name: string
  col_type: ColumnType
  alignment: Alignment
  is_default: boolean
  filter_info: FilterInfo
}

export interface MetadataResponse {
  domain: string
  columns: ColumnMetadata[]
}

export interface ColumnState extends ColumnMetadata {
  visible: boolean
  sort_direction: SortDirection
  position: number
}

export interface ActiveFilter {
  column_name: string
  display_name: string
  filter_value: string
}

export interface ColumnSpec {
  name: string
  visible: boolean
  sort_direction: SortDirection
  position: number
}

export interface FilterSpec {
  column: string
  value: string
}

export interface QueryRequest {
  columns: ColumnSpec[]
  filters: FilterSpec[]
  limit: number
  offset?: number
}

export interface QueryResponse {
  columns: string[]
  rows: string[][]
  total: number
}

export interface ValidateFilterRequest {
  domain: string
  column: string
  value: string
}

export interface ValidateFilterResponse {
  valid: boolean
  error?: string
}
