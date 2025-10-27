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

// Scan Manager Types

export type ScanStatus = 'running' | 'cancelling' | 'stopped' | 'completed' | 'error'

export interface ScanProgress {
  current: number
  total: number
  percentage: number
}

export interface ThreadState {
  thread_index: number
  status: 'idle' | 'active'
  current_file: string | null
}

export interface ThreadOperation {
  type: 'idle' | 'hashing' | 'validating'
  file?: string
}

export interface ThreadStateRaw {
  thread_index: number
  operation: ThreadOperation
}

export interface ScanPhaseInfo {
  name: 'scanning' | 'sweeping' | 'analyzing'
}

export interface ScanningProgress {
  files_scanned: number
  directories_scanned: number
}

export interface OverallProgress {
  completed: number
  total: number
  percentage: number
}

export interface ScanStatusInfo {
  status: ScanStatus
  message?: string
}

export interface ScanState {
  scan_id: number
  root_path: string
  current_phase?: ScanPhaseInfo
  completed_phases?: string[]
  scanning_progress?: ScanningProgress
  overall_progress?: OverallProgress
  thread_states?: ThreadStateRaw[]
  status?: ScanStatusInfo
  error?: string
}

export interface ScanData {
  scan_id: number
  root_path: string
  phase: number // 1=scanning, 2=sweeping, 3=analyzing
  progress: ScanProgress
  threads: ThreadState[]
  status?: ScanStatusInfo
  error_message?: string
  completed_phases?: string[] // Breadcrumbs for completed phases
  scanning_counts?: { files: number; directories: number } // File/directory counts for phase 1
}

export interface CurrentScanInfo {
  scan_id: number
  root_path: string
  scan_time: string
}

// Root with Scan Info (for Scan page)
export interface LastScanInfo {
  scan_id: number
  state: string // 'Pending' | 'Scanning' | 'Sweeping' | 'Analyzing' | 'Completed' | 'Error' | 'Stopped'
  scan_time: string
  scan_time_display: string
  error?: string
}

export interface RootWithScan {
  root_id: number
  root_path: string
  last_scan?: LastScanInfo
}

// Scan initiation
export interface InitiateScanRequest {
  root_id: number
  hash_mode: 'All' | 'New' | 'None'
  validate_mode: 'All' | 'New' | 'None'
}

export interface InitiateScanResponse {
  scan_id: number
}
