import { SettingsContent } from '../setup/SettingsContent'

export function SettingsPage() {
  return (
    <div className="flex flex-col gap-6">
      <h1 className="text-2xl font-semibold">Settings</h1>
      <SettingsContent />
    </div>
  )
}
