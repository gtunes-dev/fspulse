/**
 * Reusable scan options fields component
 * Used in both manual scan dialogs and schedule creation dialogs
 */

interface ScanOptionsFieldsProps {
  hashMode: string
  validateMode: string
  onHashModeChange: (mode: string) => void
  onValidateModeChange: (mode: string) => void
}

export function ScanOptionsFields({
  hashMode,
  validateMode,
  onHashModeChange,
  onValidateModeChange,
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

      {/* Validate Mode */}
      <div className="space-y-4">
        <label className="text-sm font-semibold">Validate Files</label>
        <div className="space-y-2">
          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="validate-mode"
              value="All"
              checked={validateMode === 'All'}
              onChange={(e) => onValidateModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">All</span>
          </label>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="validate-mode"
              value="New or Changed"
              checked={validateMode === 'New or Changed'}
              onChange={(e) => onValidateModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">New or Changed</span>
          </label>

          <label className="flex items-center gap-2 cursor-pointer">
            <input
              type="radio"
              name="validate-mode"
              value="None"
              checked={validateMode === 'None'}
              onChange={(e) => onValidateModeChange(e.target.value)}
              className="w-4 h-4"
            />
            <span className="text-sm">None</span>
          </label>
        </div>
      </div>
    </div>
  )
}
