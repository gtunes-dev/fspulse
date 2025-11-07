/**
 * Format file sizes with both decimal (KB) and binary (KiB) representations
 * for cross-platform clarity
 */

/**
 * Format bytes showing both decimal (1000-based) and binary (1024-based) units
 * Example: 754451 bytes -> "754 KB (737 KiB)"
 */
export function formatFileSize(bytes: number | null): string {
  if (bytes === null) return 'N/A'
  if (bytes === 0) return '0 B'

  // Calculate decimal (SI) units (1000-based) - matches macOS Finder
  const decimalUnits = ['B', 'KB', 'MB', 'GB', 'TB']
  let decimalSize = bytes
  let decimalIndex = 0
  while (decimalSize >= 1000 && decimalIndex < decimalUnits.length - 1) {
    decimalSize /= 1000
    decimalIndex++
  }

  // Calculate binary (IEC) units (1024-based) - matches Windows, traditional Linux
  const binaryUnits = ['B', 'KiB', 'MiB', 'GiB', 'TiB']
  let binarySize = bytes
  let binaryIndex = 0
  while (binarySize >= 1024 && binaryIndex < binaryUnits.length - 1) {
    binarySize /= 1024
    binaryIndex++
  }

  // For bytes, just show once (they're the same)
  if (decimalIndex === 0 && binaryIndex === 0) {
    return `${bytes} B`
  }

  // Show both: decimal (user-friendly) and binary (technical)
  const decimalPart = `${decimalSize.toFixed(1)} ${decimalUnits[decimalIndex]}`
  const binaryPart = `${binarySize.toFixed(2)} ${binaryUnits[binaryIndex]}`

  return `${decimalPart} (${binaryPart})`
}
