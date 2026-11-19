import { createContext, useContext } from 'react'
import type { TaskData } from '@/lib/types'

// ── Stable context (changes at task boundaries, not every progress tick) ──

export interface TaskContextType {
  currentTaskId: number | null
  activeRootId: number | null
  isRunning: boolean
  isExclusive: boolean
  isPaused: boolean
  pauseUntil: number | null
  lastTaskCompletedAt: number | null
  lastTaskScheduledAt: number | null
  backendConnected: boolean | null
  stopTask: (taskId: number) => Promise<void>
  pauseTasks: (durationSeconds: number) => Promise<void>
  unpauseTasks: () => Promise<void>
  notifyTaskScheduled: () => void
}

export const TaskContext = createContext<TaskContextType | null>(null)

// ── Progress context (updates every ~250ms during active tasks) ──

export interface TaskProgressContextType {
  activeTask: TaskData | null
}

export const TaskProgressContext = createContext<TaskProgressContextType>({ activeTask: null })

// ── Hooks ──

export function useTaskContext() {
  const context = useContext(TaskContext)
  if (!context) {
    throw new Error('useTaskContext must be used within TaskProvider')
  }
  return context
}

export function useTaskProgress() {
  return useContext(TaskProgressContext)
}
