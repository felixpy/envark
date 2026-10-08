import { displayPath, displayDiagnostic } from '@/lib/paths'
import { useState } from 'react'
import { AlertTriangle, CheckCircle2, ShieldCheck, Trash2, XCircle } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes, type Plan } from '@/domain'
import { Button } from './ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog'
import { Progress } from './ui/progress'
import { Spinner } from './shared'

function WorktreeChangeList({
  files,
}: {
  files: NonNullable<Plan['worktreeChanges']>[number]['files']
}) {
  const { t } = useStore()
  return (
    <div className="space-y-2 pt-2">
      <p className="font-medium text-amber-700 dark:text-amber-400">
        {t('存在未提交或未跟踪的变更', 'Uncommitted or untracked changes')} · {files.length}
      </p>
      <ul className="space-y-2 rounded-md bg-muted p-2">
        {files.map((file, index) => (
          <li key={index} className="flex items-start justify-between gap-3 text-xs">
            <span className="min-w-0 whitespace-pre-wrap break-all font-mono">
              {file.originalPath ? `${file.originalPath} → ${file.path}` : file.path}
            </span>
            <span className="shrink-0 text-muted-foreground">
              {file.status === '??' ? (
                t('未跟踪', 'Untracked')
              ) : file.status.includes('U') || ['AA', 'DD'].includes(file.status) ? (
                t('冲突', 'Conflict')
              ) : (
                <>
                  {file.status.includes('R')
                    ? t('重命名', 'Renamed')
                    : file.status.includes('D')
                      ? t('删除', 'Deleted')
                      : file.status.includes('A')
                        ? t('新增', 'Added')
                        : t('修改', 'Modified')}
                  {' · '}
                  {file.status[0] !== ' ' ? t('已暂存', 'Staged') : t('未暂存', 'Unstaged')}
                  {file.status[0] !== ' ' &&
                    file.status[1] !== ' ' &&
                    ` + ${t('未暂存', 'Unstaged')}`}
                </>
              )}
            </span>
          </li>
        ))}
      </ul>
    </div>
  )
}

export function OperationDialog() {
  const s = useStore()
  const { plan, result, t } = s
  const [discardPlanId, setDiscardPlanId] = useState<string | null>(null)
  const discardChanges = !!plan && discardPlanId === plan.id
  const changes = plan?.worktreeChanges ?? []
  const changedCount = changes.length
  const removeCount = (plan?.items.length ?? 0) - changedCount
  return (
    <Dialog
      open={!!plan}
      onOpenChange={(open) => {
        if (!open) s.closePlan()
      }}
    >
      <DialogContent className="max-h-[85vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>
            {result ? t('操作结果', 'Operation results') : t('审阅操作', 'Review operation')}
          </DialogTitle>
          <DialogDescription>
            {result
              ? t('每一项显示实际执行结果。', 'Each item shows its actual result.')
              : t(
                  '核对路径、影响和恢复方式后执行。',
                  'Check the paths, impact, and recovery instructions before proceeding.',
                )}
          </DialogDescription>
        </DialogHeader>
        {result ? (
          <div className="space-y-3">
            {result.items.map((item, index) => (
              <div key={index} className="flex gap-3 rounded-lg border p-3">
                {item.status === 'success' ? (
                  <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
                ) : item.status === 'skipped' ? (
                  <ShieldCheck className="size-4 shrink-0 text-muted-foreground" />
                ) : (
                  <XCircle className="size-4 shrink-0 text-destructive" />
                )}
                <div className="min-w-0 text-sm">
                  <div className="font-medium">{item.title}</div>
                  {item.status === 'skipped' && (
                    <p className="text-muted-foreground">{t('已保留', 'Kept')}</p>
                  )}
                  <pre className="mt-1 whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
                    {displayDiagnostic(item.message)}
                  </pre>
                </div>
              </div>
            ))}
            <p className="text-sm">
              {t('已移除的逻辑大小', 'Logical size removed')}:{' '}
              <span className="font-mono text-emerald-600">{formatBytes(result.removedBytes)}</span>
            </p>
            <p className="text-xs text-muted-foreground">
              {t(
                '实际磁盘释放量未测量。回收站、硬链接和共享文件可能继续占用空间。',
                'Physical disk reclamation is unmeasured. Trash, hard links, and shared files can retain disk space.',
              )}
            </p>
            {result.cancelled && (
              <p className="text-sm text-amber-600">
                {t(
                  '已取消。已完成的操作不会回滚。',
                  'Cancelled. Completed items are not rolled back.',
                )}
              </p>
            )}
          </div>
        ) : (
          <>
            <div className="max-h-72 divide-y overflow-y-auto rounded-lg border">
              {plan?.items.map((item, index) => {
                const changed = changes.find((change) => change.path === item.path)
                return (
                  <div className="space-y-1 p-3 text-sm" key={index}>
                    <div className="flex justify-between gap-4 font-medium">
                      <span>{item.title}</span>
                      {item.bytes > 0 && (
                        <span className="font-mono">{formatBytes(item.bytes)}</span>
                      )}
                    </div>
                    {item.path && (
                      <p className="break-all font-mono text-xs text-muted-foreground">
                        {displayPath(item.path)}
                      </p>
                    )}
                    {changed && !discardChanges && (
                      <p className="text-muted-foreground">
                        {t('将保留此工作树', 'This worktree will be kept')}
                      </p>
                    )}
                    {changed && <WorktreeChangeList files={changed.files} />}
                    {item.command && (!changed || discardChanges) && (
                      <pre className="whitespace-pre-wrap break-all rounded bg-muted p-2 text-xs">
                        {item.command}
                      </pre>
                    )}
                    {item.restore && (
                      <p className="text-xs text-muted-foreground">{item.restore}</p>
                    )}
                  </div>
                )
              })}
            </div>
            {changedCount > 0 && (
              <fieldset className="space-y-3 rounded-lg border p-3 text-sm" disabled={s.busy}>
                <legend className="px-1 font-medium">
                  {t('如何处理有变更的工作树', 'Handle worktrees with changes')}
                </legend>
                <label className="flex cursor-pointer items-start gap-2">
                  <input
                    type="radio"
                    name="worktree-changes"
                    checked={!discardChanges}
                    onChange={() => setDiscardPlanId(null)}
                    className="mt-1 accent-primary"
                  />
                  {t(
                    '保留有变更的工作树，仅移除其他工作树',
                    'Keep changed worktrees; remove the others',
                  )}
                </label>
                <label className="flex cursor-pointer items-start gap-2">
                  <input
                    type="radio"
                    name="worktree-changes"
                    checked={discardChanges}
                    onChange={() => setDiscardPlanId(plan?.id ?? null)}
                    className="mt-1 accent-destructive"
                  />
                  {t(
                    '丢弃列出的变更，移除所有选中的工作树',
                    'Discard listed changes and remove all selected worktrees',
                  )}
                </label>
                <p
                  className={
                    discardChanges ? 'text-xs text-destructive' : 'text-xs text-muted-foreground'
                  }
                >
                  {t(
                    '强制移除会永久丢弃未提交和未跟踪的内容，不进入回收站，也不会自动暂存。',
                    'Force removal permanently discards uncommitted and untracked content without Trash or an automatic stash.',
                  )}
                </p>
              </fieldset>
            )}
            {plan?.warnings.map((warning) => (
              <div
                key={warning}
                className="flex gap-2 rounded-lg bg-muted p-3 text-xs text-muted-foreground"
              >
                <AlertTriangle className="size-4 shrink-0" />
                {warning}
              </div>
            ))}
            {s.busy && (
              <div className="space-y-2">
                <p className="flex items-center gap-2 text-sm">
                  <Spinner />
                  {s.progress?.message ?? t('执行中…', 'Working…')}
                </p>
                <Progress
                  value={
                    s.progress?.total ? (s.progress.completed / s.progress.total) * 100 : undefined
                  }
                />
              </div>
            )}
          </>
        )}
        <DialogFooter>
          {s.busy ? (
            <Button variant="outline" onClick={() => void s.cancel()}>
              {t('取消操作', 'Cancel operation')}
            </Button>
          ) : result ? (
            <Button onClick={s.closePlan}>{t('完成', 'Done')}</Button>
          ) : (
            <>
              <Button variant="outline" onClick={s.closePlan}>
                {t('取消', 'Cancel')}
              </Button>
              <Button
                variant={changedCount > 0 && discardChanges ? 'destructive' : 'default'}
                onClick={() => {
                  if (changedCount > 0 && !discardChanges && removeCount === 0) s.closePlan()
                  else void s.execute(discardChanges)
                }}
              >
                {changedCount > 0 && !discardChanges && removeCount === 0 ? (
                  <ShieldCheck />
                ) : (
                  <Trash2 />
                )}
                {changedCount > 0
                  ? discardChanges
                    ? t('丢弃变更并全部移除', 'Discard changes and remove all')
                    : removeCount === 0
                      ? t('保留所有工作树', 'Keep all worktrees')
                      : `${t('移除', 'Remove')} ${removeCount}${t(' 个，保留 ', '; keep ')}${changedCount}${t(' 个', '')}`
                  : t('确认执行', 'Confirm operation')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
