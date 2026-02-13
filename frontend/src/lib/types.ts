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

// Task Progress Types (WebSocket protocol)

export type TaskType = 'scan'

export type TaskStatus = 'pending' | 'running' | 'pausing' | 'stopping' | 'stopped' | 'completed' | 'error'

export type BroadcastMessage =
  | { type: 'active_task'; task: TaskProgressState }
  | { type: 'no_active_task' }
  | { type: 'paused'; pause_until: number }

export interface ProgressBar {
  percentage: number | null
  message: string | null
}

export interface ThreadState {
  status: string
  status_style: string
  detail: string | null
}

export interface TaskProgressState {
  task_id: number
  task_type: TaskType
  active_root_id: number | null
  action: string
  target: string
  status: TaskStatus
  error_message: string | null
  breadcrumbs: string[] | null
  phase: string | null
  progress_bar: ProgressBar | null
  thread_states: ThreadState[] | null
}

// Pause-related types
export interface PauseState {
  paused: boolean
  pauseUntil: number  // -1 for indefinite, or unix timestamp
}

export interface PauseRequest {
  duration_seconds: number  // -1 for indefinite
}

export interface TaskData {
  task_id: number
  task_type: TaskType
  active_root_id: number | null
  action: string
  target: string
  status: TaskStatus
  error_message: string | null
  breadcrumbs: string[]
  phase: string | null
  progress_bar: ProgressBar | null
  thread_states: ThreadState[]
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
export type AlertTypeValue = 'H' | 'I' | 'A' // Suspicious Hash, Invalid Item, Access Denied
export type ContextFilterType = 'all' | 'root' | 'scan'

export interface UpdateAlertStatusRequest {
  status: AlertStatusValue
}

export interface UpdateAlertStatusResponse {
  success: boolean
}
