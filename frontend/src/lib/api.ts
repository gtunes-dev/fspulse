// API client functions for backend communication

import type {
  MetadataResponse,
  QueryRequest,
  ValidateFilterRequest,
  ValidateFilterResponse,
  UpdateAlertStatusRequest,
  UpdateAlertStatusResponse,
  BulkUpdateAlertStatusRequest,
  BulkUpdateAlertStatusByFilterRequest,
  BulkUpdateAlertStatusResponse,
} from './types'

const API_BASE = '/api'

class ApiError extends Error {
  status?: number
  statusText?: string

  constructor(
    message: string,
    status?: number,
    statusText?: string
  ) {
    super(message)
    this.name = 'ApiError'
    this.status = status
    this.statusText = statusText
  }
}

async function handleResponse<T>(response: Response): Promise<T> {
  if (!response.ok) {
    throw new ApiError(
      `API request failed: ${response.statusText}`,
      response.status,
      response.statusText
    )
  }
  return response.json()
}

/**
 * Fetch column metadata for a given domain (roots, scans, items, changes, alerts)
 */
export async function fetchMetadata(domain: string): Promise<MetadataResponse> {
  const response = await fetch(`${API_BASE}/query/${domain}/metadata`)
  return handleResponse<MetadataResponse>(response)
}

/**
 * Count matching rows for a query (efficient - no data fetch)
 */
export async function countQuery(
  domain: string,
  request: Omit<QueryRequest, 'limit' | 'offset'>
): Promise<{ count: number }> {
  const response = await fetch(`${API_BASE}/query/${domain}/count`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify({ ...request, limit: 0, offset: 0 }),
  })
  return handleResponse<{ count: number }>(response)
}

/**
 * Fetch a page of query results (efficient - no count)
 */
export async function fetchQuery(
  domain: string,
  request: QueryRequest
): Promise<{ columns: string[]; rows: string[][] }> {
  const response = await fetch(`${API_BASE}/query/${domain}/fetch`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<{ columns: string[]; rows: string[][] }>(response)
}

/**
 * Validate a filter value for a given column in a domain
 */
export async function validateFilter(
  request: ValidateFilterRequest
): Promise<ValidateFilterResponse> {
  const response = await fetch(`${API_BASE}/validate-filter`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<ValidateFilterResponse>(response)
}

/**
 * Update the status of an alert
 */
export async function updateAlertStatus(
  alertId: number,
  request: UpdateAlertStatusRequest
): Promise<UpdateAlertStatusResponse> {
  const response = await fetch(`${API_BASE}/alerts/${alertId}/status`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<UpdateAlertStatusResponse>(response)
}

/**
 * Bulk update the status of multiple alerts by their IDs
 */
export async function bulkUpdateAlertStatus(
  request: BulkUpdateAlertStatusRequest
): Promise<BulkUpdateAlertStatusResponse> {
  const response = await fetch(`${API_BASE}/alerts/bulk-status`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<BulkUpdateAlertStatusResponse>(response)
}

/**
 * Bulk update the status of all alerts matching filter criteria
 */
export async function bulkUpdateAlertStatusByFilter(
  request: BulkUpdateAlertStatusByFilterRequest
): Promise<BulkUpdateAlertStatusResponse> {
  const response = await fetch(`${API_BASE}/alerts/bulk-status-by-filter`, {
    method: 'PUT',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<BulkUpdateAlertStatusResponse>(response)
}

// ---- Integrity API ----

export interface IntegrityItem {
  item_id: number
  item_path: string
  item_name: string
  file_extension: string | null
  do_not_validate: boolean
  item_version: number
  val_state: number | null
  val_acknowledged_at: number | null
  hash_state: number | null
  hash_acknowledged_at: number | null
  first_detected_at: number
}

export interface IntegrityListResponse {
  items: IntegrityItem[]
  total: number
  offset: number
  limit: number
}

export interface IntegrityQueryParams {
  root_id: number
  issue_type?: string
  extensions?: string
  status?: string
  path_search?: string
  offset?: number
  limit?: number
}

export async function fetchIntegrity(
  params: IntegrityQueryParams
): Promise<IntegrityListResponse> {
  const qs = new URLSearchParams()
  qs.set('root_id', String(params.root_id))
  if (params.issue_type) qs.set('issue_type', params.issue_type)
  if (params.extensions) qs.set('extensions', params.extensions)
  if (params.status) qs.set('status', params.status)
  if (params.path_search) qs.set('path_search', params.path_search)
  if (params.offset !== undefined) qs.set('offset', String(params.offset))
  if (params.limit !== undefined) qs.set('limit', String(params.limit))
  const response = await fetch(`${API_BASE}/integrity?${qs}`)
  return handleResponse<IntegrityListResponse>(response)
}

export async function acknowledgeIntegrity(
  itemId: number,
  itemVersion: number,
  acknowledgeVal: boolean,
  acknowledgeHash: boolean
): Promise<{ success: boolean }> {
  const response = await fetch(`${API_BASE}/integrity/acknowledge`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      item_id: itemId,
      item_version: itemVersion,
      acknowledge_val: acknowledgeVal,
      acknowledge_hash: acknowledgeHash,
    }),
  })
  return handleResponse<{ success: boolean }>(response)
}

export async function setDoNotValidate(
  itemId: number,
  doNotValidate: boolean
): Promise<{ success: boolean }> {
  const response = await fetch(`${API_BASE}/integrity/do-not-validate`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ item_id: itemId, do_not_validate: doNotValidate }),
  })
  return handleResponse<{ success: boolean }>(response)
}

/**
 * Delete a root and all associated data (scans, items, changes, alerts)
 */
export async function deleteRoot(rootId: number): Promise<void> {
  const response = await fetch(`${API_BASE}/roots/${rootId}`, {
    method: 'DELETE',
  })

  if (!response.ok) {
    const errorData = await response.json().catch(() => ({ error: response.statusText }))
    throw new ApiError(
      errorData.error || `Failed to delete root: ${response.statusText}`,
      response.status,
      response.statusText
    )
  }
}
