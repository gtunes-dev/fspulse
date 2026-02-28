import React, { createContext, useContext, useState, useEffect, useRef, useCallback } from 'react'
import type {
  BroadcastMessage,
  TaskData,
  PauseState,
  TaskStatus,
} from '@/lib/types'

interface TaskContextType {
  activeTask: TaskData | null
  currentTaskId: number | null
  activeRootId: number | null
  isRunning: boolean
  isExclusive: boolean
  isPaused: boolean
  pauseUntil: number | null
  lastTaskCompletedAt: number | null
  lastTaskScheduledAt: number | null
  backendConnected: boolean
  stopTask: (taskId: number) => Promise<void>
  pauseTasks: (durationSeconds: number) => Promise<void>
  unpauseTasks: () => Promise<void>
  notifyTaskScheduled: () => void
}

const TaskContext = createContext<TaskContextType | null>(null)

const TERMINAL_STATUSES: TaskStatus[] = ['completed', 'stopped', 'error']

// API endpoints
const WS_PROGRESS_ENDPOINT = '/ws/tasks/progress'
const API_STOP_ENDPOINT = (taskId: number) => `/api/tasks/${taskId}/stop`

export function TaskProvider({ children }: { children: React.ReactNode }) {
  const [activeTask, setActiveTask] = useState<TaskData | null>(null)
  const [pauseState, setPauseState] = useState<PauseState | null>(null)
  const [lastTaskCompletedAt, setLastTaskCompletedAt] = useState<number | null>(null)
  const [lastTaskScheduledAt, setLastTaskScheduledAt] = useState<number | null>(null)
  const [backendConnected, setBackendConnected] = useState(false)

  const wsRef = useRef<WebSocket | null>(null)
  const reconnectTimerRef = useRef<number | null>(null)
  const lastProcessedState = useRef<{ task_id: number | null; status: TaskStatus | null }>({ task_id: null, status: null })

  // Derived state
  const currentTaskId = activeTask?.task_id ?? null
  const activeRootId = activeTask?.active_root_id ?? null
  const isRunning = activeTask !== null
  const isExclusive = activeTask?.is_exclusive ?? false
  const isPaused = pauseState?.paused ?? false
  const pauseUntil = pauseState?.pauseUntil ?? null

  const handleBroadcastMessage = useCallback((message: BroadcastMessage) => {
    if (message.type === 'paused') {
      setPauseState({ paused: true, pauseUntil: message.pause_until })
      setActiveTask(null)
      return
    }

    if (message.type === 'no_active_task') {
      setActiveTask(prev => {
        if (prev === null) return null
        setLastTaskCompletedAt(Date.now())
        lastProcessedState.current = { task_id: null, status: null }
        return null
      })
      setPauseState(null)
      return
    }

    const task = message.task
    const isTerminal = TERMINAL_STATUSES.includes(task.status)

    // Deduplicate terminal states
    if (isTerminal) {
      const last = lastProcessedState.current
      if (last.task_id === task.task_id && last.status === task.status) {
        return
      }
      lastProcessedState.current = { task_id: task.task_id, status: task.status }
    }

    setActiveTask(prev => {
      if (prev !== null && prev.task_id !== task.task_id) {
        setLastTaskCompletedAt(Date.now())
      }

      if (isTerminal) {
        setLastTaskCompletedAt(Date.now())
      }

      return {
        task_id: task.task_id,
        task_type: task.task_type,
        active_root_id: task.active_root_id,
        is_exclusive: task.is_exclusive,
        is_stoppable: task.is_stoppable,
        is_pausable: task.is_pausable,
        action: task.action,
        target: task.target,
        status: task.status,
        error_message: task.error_message,
        breadcrumbs: task.breadcrumbs ?? [],
        phase: task.phase,
        progress_bar: task.progress_bar,
        thread_states: task.thread_states ?? [],
      }
    })
  }, [])

  const connectWebSocket = useCallback(() => {
    if (wsRef.current?.readyState === WebSocket.OPEN) return
    if (wsRef.current) wsRef.current.close()

    const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:'
    const ws = new WebSocket(`${protocol}//${window.location.host}${WS_PROGRESS_ENDPOINT}`)
    wsRef.current = ws

    ws.onopen = () => {
      setBackendConnected(true)
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current)
        reconnectTimerRef.current = null
      }
    }

    ws.onmessage = (event) => {
      try {
        handleBroadcastMessage(JSON.parse(event.data))
      } catch (error) {
        console.error('Failed to parse WebSocket message:', error)
      }
    }

    ws.onerror = (error) => console.error('WebSocket error:', error)

    ws.onclose = () => {
      setBackendConnected(false)
      if (!reconnectTimerRef.current) {
        reconnectTimerRef.current = window.setTimeout(() => {
          reconnectTimerRef.current = null
          connectWebSocket()
        }, 2000)
      }
    }
  }, [handleBroadcastMessage])

  const stopTask = useCallback(async (taskId: number) => {
    const response = await fetch(API_STOP_ENDPOINT(taskId), { method: 'POST' })
    if (!response.ok) throw new Error('Failed to stop task')
  }, [])

  const pauseTasks = useCallback(async (durationSeconds: number) => {
    const response = await fetch('/api/pause', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ duration_seconds: durationSeconds })
    })
    if (!response.ok) throw new Error('Failed to pause')
  }, [])

  const unpauseTasks = useCallback(async () => {
    const response = await fetch('/api/pause', { method: 'DELETE' })
    if (!response.ok) {
      const text = await response.text()
      throw new Error(text || 'Failed to unpause')
    }
  }, [])

  const notifyTaskScheduled = useCallback(() => {
    setLastTaskScheduledAt(Date.now())
  }, [])

  useEffect(() => {
    connectWebSocket()
    return () => {
      wsRef.current?.close()
      if (reconnectTimerRef.current) clearTimeout(reconnectTimerRef.current)
    }
  }, [connectWebSocket])

  const value: TaskContextType = {
    activeTask,
    currentTaskId,
    activeRootId,
    isRunning,
    isExclusive,
    isPaused,
    pauseUntil,
    lastTaskCompletedAt,
    lastTaskScheduledAt,
    backendConnected,
    stopTask,
    pauseTasks,
    unpauseTasks,
    notifyTaskScheduled,
  }

  return (
    <TaskContext.Provider value={value}>
      {children}
    </TaskContext.Provider>
  )
}

export function useTaskContext() {
  const context = useContext(TaskContext)
  if (!context) {
    throw new Error('useTaskContext must be used within TaskProvider')
  }
  return context
}
