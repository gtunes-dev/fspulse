import React, { createContext, useContext, useState, useEffect, useRef, useCallback } from 'react'
import type {
  BroadcastMessage,
  ScanData,
  ScanState,
} from '@/lib/types'

interface ScanManagerContextType {
  activeScans: Map<number, ScanData>
  currentScanId: number | null
  isScanning: boolean
  lastScanCompletedAt: number | null
  lastScanScheduledAt: number | null
  stopScan: (scanId: number) => Promise<void>
  notifyScanScheduled: () => void
}

const ScanManagerContext = createContext<ScanManagerContextType | null>(null)

export function ScanManagerProvider({ children }: { children: React.ReactNode }) {
  const [activeScans, setActiveScans] = useState<Map<number, ScanData>>(new Map())
  const [currentScanId, setCurrentScanId] = useState<number | null>(null)
  const [lastScanCompletedAt, setLastScanCompletedAt] = useState<number | null>(null)
  const [lastScanScheduledAt, setLastScanScheduledAt] = useState<number | null>(null)

  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimerRef = useRef<number | null>(null)
  const scanPhaseRef = useRef<number>(1)
  const scanningCountsRef = useRef({ files: 0, directories: 0 })
  const phaseBreadcrumbsRef = useRef<string[]>([])
  const lastProcessedState = useRef<{ scan_id: number | null; status: string | null }>({ scan_id: null, status: null })
  const completionTimeouts = useRef<Map<number, number>>(new Map())
  const currentScanIdRef = useRef<number | null>(currentScanId)

  // Keep ref in sync with state
  useEffect(() => {
    currentScanIdRef.current = currentScanId
  }, [currentScanId])

  const isScanning = currentScanId !== null

  // Process broadcast messages received from backend via WebSocket
  const handleBroadcastMessage = useCallback((message: BroadcastMessage) => {
    // Handle no_active_scan message
    if (message.type === 'no_active_scan') {
      // If we have no active scan, ignore (already idle)
      if (currentScanIdRef.current === null) {
        return
      }

      // Clear active scan (handles reconnect desync)
      console.log('[ScanManager] Received no_active_scan, clearing state')
      setCurrentScanId(null)
      setActiveScans(new Map())
      lastProcessedState.current = { scan_id: null, status: null }

      // Cancel any pending completion timeouts
      if (currentScanIdRef.current !== null) {
        const timeoutId = completionTimeouts.current.get(currentScanIdRef.current)
        if (timeoutId) {
          clearTimeout(timeoutId)
          completionTimeouts.current.delete(currentScanIdRef.current)
        }
      }
      return
    }

    // Handle active_scan message
    const state = message.scan
    const scanId = state.scan_id
    const statusValue = state.status?.status

    // Validate scan ID
    if (!scanId) {
      console.error('[ScanManager] Received active_scan with null scan_id')
      return
    }

    // Deduplicate terminal states only (completed, stopped, error)
    // Running states need all updates for progress tracking
    const isTerminalState = statusValue && ['completed', 'stopped', 'error'].includes(statusValue)
    if (isTerminalState) {
      if (lastProcessedState.current.scan_id === scanId && lastProcessedState.current.status === statusValue) {
        console.log('[ScanManager] Deduplicating terminal state:', { scanId, status: statusValue })
        return
      }
      console.log('[ScanManager] Processing terminal state:', { scanId, status: statusValue })
      // Update last processed state for terminal states only
      lastProcessedState.current = { scan_id: scanId, status: statusValue || null }
    }

    // Handle error in state
    if (state.error) {
      console.error('Scan error:', state.error)
    }

    setActiveScans(prevScans => {
      const newScans = new Map(prevScans)

      const scanData: ScanData = newScans.get(scanId) || {
        scan_id: scanId,
        root_path: state.root_path,
        phase: 1,
        progress: { current: 0, total: 1, percentage: 0 },
        threads: []
      }

      // Map phase name to phase number
      if (state.current_phase) {
        if (state.current_phase.name === 'scanning') {
          scanData.phase = 1
          scanPhaseRef.current = 1
        } else if (state.current_phase.name === 'sweeping') {
          scanData.phase = 2
          scanPhaseRef.current = 2
          // Preserve scanning counts from phase 1 for phase 2
          scanData.scanning_counts = { ...scanningCountsRef.current }
        } else if (state.current_phase.name === 'analyzing') {
          scanData.phase = 3
          scanPhaseRef.current = 3
        }
      }

      // Update completed phase breadcrumbs
      phaseBreadcrumbsRef.current = state.completed_phases || []
      scanData.completed_phases = state.completed_phases || []

      // Update scanning progress (phase 1)
      if (state.scanning_progress) {
        scanningCountsRef.current = {
          files: state.scanning_progress.files_scanned || 0,
          directories: state.scanning_progress.directories_scanned || 0
        }
        scanData.scanning_counts = { ...scanningCountsRef.current }
      }

      // Update overall progress (phase 3)
      if (state.overall_progress) {
        scanData.progress = {
          current: state.overall_progress.completed,
          total: state.overall_progress.total,
          percentage: state.overall_progress.percentage
        }
      }

      // Update thread states
      if (state.thread_states && state.thread_states.length > 0) {
        scanData.threads = state.thread_states.map(thread => {
          if (thread.operation.type === 'idle') {
            return { thread_index: thread.thread_index, status: 'idle', current_file: null }
          }

          let prefix = ''
          let file = ''
          if (thread.operation.type === 'hashing') {
            prefix = 'Hashing:'
            file = thread.operation.file || ''
          } else if (thread.operation.type === 'validating') {
            prefix = 'Validating:'
            file = thread.operation.file || ''
          }

          return {
            thread_index: thread.thread_index,
            status: 'active' as const,
            current_file: prefix ? `${prefix} ${file}` : file
          }
        })
      }

      // Update scan status
      if (state.status) {
        scanData.status = state.status
        if (state.status.message) {
          scanData.error_message = state.status.message
        }
      }

      newScans.set(scanId, scanData)

      // Update current scan ID if different
      if (currentScanIdRef.current !== scanId) {
        console.log('[ScanManager] Setting currentScanId:', { from: currentScanIdRef.current, to: scanId })
        setCurrentScanId(scanId)
      }

      // Handle scan completion/termination
      if (statusValue && ['completed', 'stopped', 'error'].includes(statusValue)) {
        // Check if we already have a timeout for this scan
        if (!completionTimeouts.current.has(scanId)) {
          console.log('[ScanManager] Creating completion timeout for scan:', scanId)
          // Notify that a scan has completed (triggers refresh in RecentScansTable)
          setLastScanCompletedAt(Date.now())

          // Display completion status for a moment before clearing
          const delay = statusValue === 'error' ? 3000 : 2000
          const timeoutId = window.setTimeout(() => {
            console.log('[ScanManager] Completion timeout fired - clearing scan:', scanId)
            setActiveScans(prev => {
              const updated = new Map(prev)
              updated.delete(scanId)
              return updated
            })
            setCurrentScanId(null)
            lastProcessedState.current = { scan_id: null, status: null }
            completionTimeouts.current.delete(scanId)
          }, delay)

          completionTimeouts.current.set(scanId, timeoutId)
        } else {
          console.log('[ScanManager] Completion timeout already exists for scan:', scanId)
        }
      }

      return newScans
    })
  }, [])

  // Establish persistent WebSocket connection to receive scan state updates
  const connectWebSocket = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) {
      console.log('WebSocket already connected')
      return
    }

    // Close stale connection if present
    if (wsRef.current) {
      wsRef.current.close()
    }

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsUrl = `${protocol}//${window.location.host}/ws/scans/progress`

    console.log('Connecting to WebSocket:', wsUrl)
    const ws = new WebSocket(wsUrl)
    wsRef.current = ws

    ws.onopen = () => {
      console.log('WebSocket connected to scan state broadcast')
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current)
        reconnectTimerRef.current = null
      }
    }

    ws.onmessage = (event) => {
      try {
        const message: BroadcastMessage = JSON.parse(event.data)
        handleBroadcastMessage(message)
      } catch (error) {
        console.error('Failed to parse WebSocket message:', error)
      }
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
    }

    ws.onclose = (event) => {
      console.log('WebSocket closed:', event.code, event.reason)

      // Automatically reconnect after brief delay
      if (!reconnectTimerRef.current) {
        reconnectTimerRef.current = window.setTimeout(() => {
          reconnectTimerRef.current = null
          console.log('Attempting to reconnect WebSocket...')
          connectWebSocket()
        }, 2000)
      }
    }
  }, [handleBroadcastMessage])

  // Stop scan
  const stopScan = useCallback(async (scanId: number) => {
    try {
      const response = await fetch(`/api/scans/${scanId}/cancel`, {
        method: 'POST'
      })

      if (!response.ok) {
        console.error('Failed to cancel scan:', response.statusText)
        throw new Error('Failed to cancel scan')
      }
    } catch (error) {
      console.error('Error cancelling scan:', error)
      throw error
    }
  }, [])

  // Notify that a scan was scheduled (triggers refresh in UpcomingScansTable)
  const notifyScanScheduled = useCallback(() => {
    setLastScanScheduledAt(Date.now())
  }, [])

  // Initialize WebSocket connection on mount
  useEffect(() => {
    connectWebSocket()

    return () => {
      if (wsRef.current) {
        wsRef.current.close()
      }
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current)
      }
      // Clear any pending completion timeouts
      completionTimeouts.current.forEach(timeoutId => clearTimeout(timeoutId))
      completionTimeouts.current.clear()
    }
  }, [connectWebSocket])

  const value: ScanManagerContextType = {
    activeScans,
    currentScanId,
    isScanning,
    lastScanCompletedAt,
    lastScanScheduledAt,
    stopScan,
    notifyScanScheduled,
  }

  return (
    <ScanManagerContext.Provider value={value}>
      {children}
    </ScanManagerContext.Provider>
  )
}

export function useScanManager() {
  const context = useContext(ScanManagerContext)
  if (!context) {
    throw new Error('useScanManager must be used within ScanManagerProvider')
  }
  return context
}
