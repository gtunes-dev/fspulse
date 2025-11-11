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

// Broadcast message types from WebSocket
export type BroadcastMessage =
  | { type: 'active_scan'; scan: ScanState }
  | { type: 'no_active_scan' }

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
  scan_id: number | null  // null when idle
  root_id?: number | null  // null when idle
  root_path: string  // empty when idle
  current_phase?: 'scanning' | 'sweeping' | 'analyzing'  // simplified from object to string
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
  started_at: string
}

// Root with Scan Info (for Scan page)
export interface LastScanInfo {
  scan_id: number
  state: string // 'Pending' | 'Scanning' | 'Sweeping' | 'Analyzing' | 'Completed' | 'Error' | 'Stopped'
  started_at: number  // Unix timestamp (seconds) for client-side formatting
  file_count?: number
  folder_count?: number
  error?: string
}

export interface RootWithScan {
  root_id: number
  root_path: string
  last_scan?: LastScanInfo
  schedule_count: number  // Number of active schedules for this root
}

// Scan scheduling
export interface ScheduleScanRequest {
  root_id: number
  hash_mode: 'All' | 'New' | 'None'
  validate_mode: 'All' | 'New' | 'None'
}

// Schedule types
export type ScheduleType = 'Daily' | 'Weekly' | 'Interval' | 'Monthly'
export type IntervalUnit = 'Minutes' | 'Hours' | 'Days' | 'Weeks'

export interface Schedule {
  schedule_id: number
  root_id: number
  enabled: boolean
  schedule_name: string
  schedule_type: ScheduleType
  time_of_day?: string  // 'HH:MM' format
  days_of_week?: string  // JSON array of day names
  day_of_month?: number  // 1-31
  interval_value?: number
  interval_unit?: IntervalUnit
  hash_mode: 'All' | 'New' | 'None'
  validate_mode: 'All' | 'New' | 'None'
  created_at: number  // Unix timestamp
  updated_at: number  // Unix timestamp
}

export interface ScheduleWithRoot extends Schedule {
  root_path: string
  next_scan_time?: number  // Unix timestamp
}

// Insights Page Types

export type AlertStatusValue = 'O' | 'F' | 'D' // Open, Flagged, Dismissed
export type AlertTypeValue = 'H' | 'I' // Suspicious Hash, Invalid Item
export type ContextFilterType = 'all' | 'root' | 'scan'

export interface UpdateAlertStatusRequest {
  status: AlertStatusValue
}

export interface UpdateAlertStatusResponse {
  success: boolean
}
