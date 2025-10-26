export function HomePage() {
  return (
    <div>
      <h2 className="text-2xl font-semibold mb-4">Dashboard</h2>
      <p className="text-muted-foreground">
        Welcome to FsPulse. Your filesystem monitoring dashboard is ready.
      </p>

      {/* Theme test elements */}
      <div className="mt-8 space-y-4">
        <div className="p-4 bg-primary text-primary-foreground rounded">
          Primary color test - This should be blue
        </div>
        <div className="p-4 bg-muted text-muted-foreground rounded">
          Muted color test - This should be light grey
        </div>
        <button className="px-4 py-2 bg-primary text-primary-foreground rounded hover:opacity-90">
          Primary Button
        </button>
      </div>
    </div>
  )
}
