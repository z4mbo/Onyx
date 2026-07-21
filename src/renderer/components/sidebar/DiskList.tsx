import { useDisks } from '@/hooks/useDisks'

interface DiskListProps {
  selectedDisk: string | null
  onSelectDisk: (mount: string) => void
}

/**
 * Lists available filesystem locations (Windows drives or macOS volumes).
 */
export default function DiskList({ selectedDisk, onSelectDisk }: DiskListProps) {
  const { disks, loading, error, refresh } = useDisks()

  if (loading) {
    return (
      <div className="px-3 py-2 text-xs text-win-text-tertiary">Loading locations...</div>
    )
  }

  if (error) {
    return (
      <div className="px-3 py-2 text-xs text-red-600">
        {error}{' '}
        <button onClick={refresh} className="underline hover:text-red-500">
          Retry
        </button>
      </div>
    )
  }

  if (disks.length === 0) {
    return (
      <div className="px-3 py-2 text-xs text-win-text-tertiary">No locations found.</div>
    )
  }

  return (
    <div className="flex flex-wrap gap-1 px-3 py-2">
      {disks.map((disk) => {
        const isActive = selectedDisk === disk.mount
        return (
          <button
            key={disk.mount}
            onClick={() => onSelectDisk(disk.mount)}
            className={`rounded-md px-2 py-1 text-xs font-medium transition-colors ${
              isActive
                ? 'bg-win-accent text-white'
                : 'bg-win-card text-win-text-secondary border border-win-border hover:bg-win-hover hover:text-win-text'
            }`}
            title={`${disk.name || disk.mount} — ${formatSize(disk.free)} free of ${formatSize(disk.size)}`}
          >
            {disk.mount}
          </button>
        )
      })}
    </div>
  )
}

function formatSize(bytes: number): string {
  if (bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(1024))
  const val = bytes / Math.pow(1024, i)
  return `${val.toFixed(1)} ${units[i]}`
}
