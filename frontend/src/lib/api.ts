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
