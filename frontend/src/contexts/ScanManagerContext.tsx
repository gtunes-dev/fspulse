import React, { createContext, useContext, useState, useEffect, useRef, useCallback } from 'react'
import type {
  ScanData,
  ScanState,
  CurrentScanInfo,
} from '@/lib/types'

interface ScanManagerContextType {
  activeScans: Map<number, ScanData>
  currentScanId: number | null
  isScanning: boolean
  lastScanCompletedAt: number | null
  connectScanWebSocket: (scanId: number, rootPath: string) => void
  stopScan: (scanId: number) => Promise<void>
  checkForActiveScan: () => Promise<void>
}

const ScanManagerContext = createContext<ScanManagerContextType | null>(null)

export function ScanManagerProvider({ children }: { children: React.ReactNode }) {
  const [activeScans, setActiveScans] = useState<Map<number, ScanData>>(new Map())
  const [currentScanId, setCurrentScanId] = useState<number | null>(null)
  const [lastScanCompletedAt, setLastScanCompletedAt] = useState<number | null>(null)

  const wsRef = useRef<WebSocket | null>(null)
  const pingIntervalRef = useRef<number | null>(null)
  const scanPhaseRef = useRef<number>(1)
  const scanningCountsRef = useRef({ files: 0, directories: 0 })
  const phaseBreadcrumbsRef = useRef<string[]>([])
  const completedScansRef = useRef<Set<number>>(new Set())

  const isScanning = currentScanId !== null

  // Handle WebSocket state updates
  const handleStateUpdate = useCallback((state: ScanState) => {
    if (state.error) {
      console.error('WebSocket error:', state.error)
      return
    }

    setActiveScans(prevScans => {
      const newScans = new Map(prevScans)

      const scanData: ScanData = newScans.get(state.scan_id) || {
        scan_id: state.scan_id,
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

      newScans.set(state.scan_id, scanData)

      // Handle scan completion/termination
      const statusValue = state.status?.status
      if (statusValue && ['completed', 'stopped', 'error'].includes(statusValue)) {
        if (!completedScansRef.current.has(state.scan_id)) {
          completedScansRef.current.add(state.scan_id)

          // Notify that a scan has completed (triggers refresh in RecentScansTable)
          setLastScanCompletedAt(Date.now())

          // Remove scan after delay
          const delay = statusValue === 'error' ? 3000 : 2000
          setTimeout(() => {
            setActiveScans(prev => {
              const updated = new Map(prev)
              updated.delete(state.scan_id)
              return updated
            })
            completedScansRef.current.delete(state.scan_id)
            setCurrentScanId(null)
          }, delay)
        }
      }

      return newScans
    })
  }, [])

  // Connect to WebSocket
  const connectScanWebSocket = useCallback((scanId: number, rootPath: string) => {
    // Close existing connection
    if (wsRef.current) {
      wsRef.current.close()
    }

    // Clear existing ping interval
    if (pingIntervalRef.current) {
      clearInterval(pingIntervalRef.current)
      pingIntervalRef.current = null
    }

    // Initialize scan state
    setCurrentScanId(scanId)
    scanPhaseRef.current = 1
    scanningCountsRef.current = { files: 0, directories: 0 }
    phaseBreadcrumbsRef.current = []

    setActiveScans(prevScans => {
      const newScans = new Map(prevScans)
      newScans.set(scanId, {
        scan_id: scanId,
        root_path: rootPath,
        phase: 1,
        progress: {
          current: 0,
          total: 1,
          percentage: 0
        },
        threads: []
      })
      return newScans
    })

    // Connect to WebSocket
    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const wsUrl = `${protocol}//${window.location.host}/ws/scans/progress`

    const ws = new WebSocket(wsUrl)
    wsRef.current = ws

    ws.onopen = () => {
      console.log('WebSocket connected')

      // Start sending pings every 30 seconds to maintain bidirectional traffic
      // This prevents proxy/load balancer timeouts on the client->server direction
      pingIntervalRef.current = setInterval(() => {
        if (ws.readyState === WebSocket.OPEN) {
          ws.send('ping')
        }
      }, 30000)
    }

    ws.onmessage = (event) => {
      const state: ScanState = JSON.parse(event.data)
      handleStateUpdate(state)
    }

    ws.onerror = (error) => {
      console.error('WebSocket error:', error)
    }

    ws.onclose = () => {
      console.log('WebSocket closed')

      // Clear ping interval
      if (pingIntervalRef.current) {
        clearInterval(pingIntervalRef.current)
        pingIntervalRef.current = null
      }

      setTimeout(() => {
        setActiveScans(prev => {
          const updated = new Map(prev)
          updated.delete(scanId)
          return updated
        })
        setCurrentScanId(null)
      }, 2000)
    }
  }, [handleStateUpdate])

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

  // Check for active scan on mount
  const checkForActiveScan = useCallback(async () => {
    try {
      const response = await fetch('/api/scans/current')
      if (!response.ok) return

      const data: CurrentScanInfo | null = await response.json()

      if (data && data.scan_id) {
        console.log('Found active scan:', data)
        connectScanWebSocket(data.scan_id, data.root_path)
      }
    } catch (error) {
      console.error('Failed to check for active scan:', error)
    }
  }, [connectScanWebSocket])

  // Check for active scan on mount
  useEffect(() => {
    checkForActiveScan()

    return () => {
      if (wsRef.current) {
        wsRef.current.close()
      }
      if (pingIntervalRef.current) {
        clearInterval(pingIntervalRef.current)
      }
    }
  }, [checkForActiveScan])

  const value: ScanManagerContextType = {
    activeScans,
    currentScanId,
    isScanning,
    lastScanCompletedAt,
    connectScanWebSocket,
    stopScan,
    checkForActiveScan,
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
