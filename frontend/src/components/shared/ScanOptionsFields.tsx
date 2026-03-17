/**
 * Reusable scan options fields component
 * Used in both manual scan dialogs and schedule creation dialogs
 */

interface ScanOptionsFieldsProps {
  hashMode: string
  isVal: boolean
  onHashModeChange: (mode: string) => void
  onIsValChange: (isVal: boolean) => void
}

export function ScanOptionsFields({
  hashMode,
  isVal,
  onHashModeChange,
  onIsValChange,
}: ScanOptionsFieldsProps) {
  return (
    <div className="space-y-6">
      {/* Hash Mode */}
      <div className="space-y-4">
        <label className="text-sm font-semibold">Hash Files</label>
        <div className="space-y-2">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="hash-mode"
              value="All"
              checked={hashMode === 'All'}
              onChange={(e) => onHashModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">All</span>
          </label>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="hash-mode"
              value="New or Changed"
              checked={hashMode === 'New or Changed'}
              onChange={(e) => onHashModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">New or Changed</span>
          </label>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="hash-mode"
              value="None"
              checked={hashMode === 'None'}
              onChange={(e) => onHashModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">None</span>
          </label>
        </div>
      </div>

      {/* Validate Files */}
      <div className="space-y-4">
        <label className="flex items-center gap-2 cursor-pointer">
          <input
            type="checkbox"
            checked={isVal}
            onChange={(e) => onIsValChange(e.target.checked)}
            className="w-4 h-4"
          />
          <span className="text-sm font-semibold">Validate Files</span>
        </label>
      </div>
    </div>
  )
}
