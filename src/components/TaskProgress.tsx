import { useEffect, useState } from 'react'
import { useStore } from '@/store'
import { operationLabel, progressDetail, progressLabel } from '@/lib/progress'
import { displayDiagnostic } from '@/lib/paths'
import { Progress } from './ui/progress'
import { Spinner } from './shared'

export function TaskProgress() {
  const { task, progress, t } = useStore()
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    if (!task) return
    const timer = window.setInterval(() => setNow(Date.now()), 1000)
    return () => window.clearInterval(timer)
  }, [task])
  if (!task) return null
  const status = progressLabel(
    progress ??
      (['prepare', 'updates', 'caches'].includes(task.kind)
        ? { stage: task.kind, jobId: '', completed: 0, total: null, message: '' }
        : null),
    t,
    task.kind === 'execute',
  )
  const operation = operationLabel(task.request, t)
  const detail = progressDetail(progress)
  const elapsed = Math.max(0, Math.floor((now - task.startedAt) / 1000))
  const count =
    progress?.total && progress.stage !== 'execute'
      ? `${progress.completed}/${progress.total}`
      : null
  return (
    <div className="min-w-0 flex-1 space-y-2" role="status" aria-live="polite">
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <Spinner />
        {operation && <span className="font-medium">{operation}</span>}
        <span>{status}</span>
        {count && <span className="font-mono text-xs text-muted-foreground">{count}</span>}
        <span className="text-xs text-muted-foreground">
          {elapsed}
          {t(' 秒', 's')}
        </span>
      </div>
      {detail && (
        <p
          className="truncate font-mono text-xs text-muted-foreground"
          title={displayDiagnostic(detail)}
        >
          {displayDiagnostic(detail)}
        </p>
      )}
      {progress &&
        progress.total === null &&
        progress.completed > 0 &&
        [
          'prepare',
          'measure-projects',
          'measure-worktrees',
          'measure-caches',
          'refresh-affected',
        ].includes(progress.stage) && (
          <p className="text-xs text-muted-foreground">
            {t('已检查', 'Checked')} {progress.completed} {t('个条目', 'entries')}
          </p>
        )}
      <Progress
        value={progress?.total ? (progress.completed / progress.total) * 100 : undefined}
        className={progress?.total ? 'h-1' : 'h-1 animate-pulse'}
      />
    </div>
  )
}
