// API client functions for backend communication

import type { MetadataResponse, QueryRequest, QueryResponse, ValidateFilterRequest, ValidateFilterResponse } from './types'

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
  const response = await fetch(`${API_BASE}/metadata/${domain}`)
  return handleResponse<MetadataResponse>(response)
}

/**
 * Execute a query against a domain with column specifications, filters, and pagination
 */
export async function executeQuery(
  domain: string,
  request: QueryRequest
): Promise<QueryResponse> {
  const response = await fetch(`${API_BASE}/query/${domain}`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(request),
  })
  return handleResponse<QueryResponse>(response)
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
