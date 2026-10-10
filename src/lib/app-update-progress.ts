import type { AppUpdateProgress } from '@/desktop'

export function mergeAppUpdateProgress(
  previous: AppUpdateProgress,
  next: AppUpdateProgress,
): AppUpdateProgress {
  if (previous.installing) return previous
  const downloaded = Math.max(
    previous.downloaded,
    Number.isFinite(next.downloaded) ? next.downloaded : 0,
  )
  const installing = previous.installing || next.installing
  const total =
    installing && downloaded > 0
      ? downloaded
      : (previous.total ??
        (next.total !== null && Number.isFinite(next.total) && next.total > 0 ? next.total : null))
  return { downloaded, total, installing }
}
