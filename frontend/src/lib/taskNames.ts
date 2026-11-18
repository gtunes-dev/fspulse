import { HOUR } from '@/lib/timeRange'

/**
 * Task types in the user's terms. Internally an integrity check is a
 * "scan" and a full comparison of a root with what is recorded is a
 * "checkpoint"; neither word says what the task is for.
 */
export function taskTypeDisplayName(taskType: string): string {
  switch (taskType) {
    case 'scan':
      return 'Integrity check'
    case 'checkpoint':
      return 'Change check'
    case 'compact_database':
      return 'Compact Database'
    default:
      return taskType
  }
}

/**
 * A Browse link showing what changed around a task: from the hour it
 * started in to the end of the hour it finished in. Times fall on the
 * hour because that is how precisely the past is recorded.
 */
export function browseLinkForPeriod(rootId: number, startedAt: number, completedAt: number | null): string {
  const end = (completedAt ?? startedAt) + HOUR
  return `/browse?root_id=${rootId}&from=${startedAt}&to=${end}`
}
