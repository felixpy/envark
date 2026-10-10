import { displayPath } from '@/lib/paths'
import { useState } from 'react'
import { AlertTriangle, Check, ShieldCheck, Trash2 } from 'lucide-react'
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
import { TaskProgress } from './TaskProgress'
import { OperationResults } from './OperationResults'

function WorktreeChangeList({
  files,
}: {
  files: NonNullable<Plan['worktreeChanges']>[number]['files']
}) {
  const { t } = useStore()
  return (
    <div className="space-y-2 pt-2">
      <p className="font-medium text-amber-700 dark:text-amber-400">
        {t('移除前需要确认的内容', 'Content requiring removal confirmation')} · {files.length}
      </p>
      <ul className="space-y-2 rounded-md bg-muted p-2">
        {files.map((file, index) => (
          <li key={index} className="flex items-start justify-between gap-3 text-xs">
            <span className="min-w-0 whitespace-pre-wrap break-all font-mono">
              {file.originalPath ? `${file.originalPath} → ${file.path}` : file.path}
            </span>
            <span className="shrink-0 text-muted-foreground">
              {file.status === 'repository' ? (
                t('嵌套仓库及本地历史', 'Nested repository and history')
              ) : file.status === 'submodule' ? (
                t('子模块或嵌套工作树', 'Submodule or nested checkout')
              ) : file.status === '??' ? (
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
  const { plan, result, operationError, t } = s
  const preparing = s.task?.kind === 'prepare'
  const executing = s.task?.kind === 'execute'
  const [discardPlanId, setDiscardPlanId] = useState<string | null>(null)
  const discardChanges = !!plan && discardPlanId === plan.id
  const changes = plan?.worktreeChanges ?? []
  const changedCount = changes.length
  const removeCount = (plan?.items.length ?? 0) - changedCount
  return (
    <Dialog
      open={s.operationOpen}
      onOpenChange={(open) => {
        if (!open) s.closePlan()
      }}
    >
      <DialogContent
        className="flex max-h-[85vh] flex-col overflow-hidden sm:max-w-2xl"
        showCloseButton={!s.busy}
      >
        <DialogHeader className="shrink-0">
          <DialogTitle>
            {preparing
              ? t('正在准备审阅', 'Preparing review')
              : result
                ? t('操作结果', 'Operation results')
                : operationError
                  ? t('需要重新审阅', 'Review required')
                  : executing
                    ? t('正在执行操作', 'Executing operation')
                    : t('审阅操作', 'Review operation')}
          </DialogTitle>
          <DialogDescription>
            {preparing
              ? t(
                  '正在核对所选项目，准备完成后由你确认执行。',
                  'Checking selected items. You can confirm the operation when the review is ready.',
                )
              : result
                ? t('每一项显示实际执行结果。', 'Each item shows its actual result.')
                : operationError
                  ? t(
                      '重新检查当前状态后再执行。',
                      'Check the current state again before proceeding.',
                    )
                  : executing
                    ? t(
                        '正在处理已确认的项目，请查看下方进度。',
                        'Processing confirmed items. Follow the progress below.',
                      )
                    : t(
                        '核对路径、影响和恢复方式后执行。',
                        'Check the paths, impact, and recovery instructions before proceeding.',
                      )}
          </DialogDescription>
        </DialogHeader>
        <div className="min-h-0 flex-1 overflow-y-auto pr-1" data-slot="operation-scroll">
          {preparing ? (
            <TaskProgress />
          ) : operationError ? (
            <div className="space-y-3">
              <p role="alert" className="whitespace-pre-wrap break-all text-sm text-destructive">
                {operationError.includes('The cleanup policy changed.')
                  ? t(
                      '清理设置已变化，请重新审阅所选项目后确认执行。',
                      'Cleanup settings changed. Review the selected items again before confirming.',
                    )
                  : operationError.includes('plan is no longer valid')
                    ? t(
                        '操作预览已失效，请重新审阅以获取当前状态。',
                        'This preview expired. Review again to check the current state.',
                      )
                    : operationError}
              </p>
              <p className="text-sm text-muted-foreground">
                {t(
                  '点击“重新审阅”，应用会重新核对所选项目；再次确认后才会执行。',
                  'Review again to check the selected items. Execution still requires your confirmation.',
                )}
              </p>
              {s.busy && <TaskProgress />}
            </div>
          ) : result ? (
            <OperationResults result={result} />
          ) : (
            <div className="space-y-4">
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
                  {changes.some((change) =>
                    change.files.some((file) => ['repository', 'submodule'].includes(file.status)),
                  ) && (
                    <p className="text-xs text-destructive">
                      {t(
                        '强制移除也会删除列出的嵌套仓库、子模块工作目录及其中的本地内容；嵌套仓库的本地历史可能无法恢复。',
                        'Force removal also deletes listed nested repositories, submodule working directories, and local content. Local history in nested repositories may be unrecoverable.',
                      )}
                    </p>
                  )}
                </fieldset>
              )}
              {!!plan?.runtimeDependents?.length && (
                <details className="rounded-lg border p-3 text-sm">
                  <summary className="cursor-pointer font-medium">
                    {t('以下已扫描项目配置了此版本', 'Scanned projects requesting this version')} (
                    {plan.runtimeDependents.length})
                  </summary>
                  <p className="mt-2 text-xs text-muted-foreground">
                    {t(
                      '卸载后，这些项目可能需要切换版本或重新安装。项目文件不会被删除。',
                      'After uninstalling, these projects may need another version or a reinstall. Project files are not removed.',
                    )}
                  </p>
                  <ul className="mt-3 space-y-3">
                    {plan.runtimeDependents.map((project) => (
                      <li key={project.path} className="break-all text-xs">
                        <p className="font-mono">{displayPath(project.path)}</p>
                        <p className="text-muted-foreground">{project.pins.join(' · ')}</p>
                      </li>
                    ))}
                  </ul>
                </details>
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
              {s.busy && <TaskProgress />}
            </div>
          )}
        </div>
        <DialogFooter className="shrink-0 border-t pt-4">
          {s.busy ? (
            <Button variant="outline" onClick={() => void s.cancel()}>
              {t('取消操作', 'Cancel operation')}
            </Button>
          ) : result ? (
            <>
              {s.canReviewFailed && (
                <Button variant="outline" onClick={() => void s.reviewFailed()}>
                  {t('重新审阅失败项', 'Review failed items')}
                </Button>
              )}
              <Button onClick={s.closePlan}>{t('完成', 'Done')}</Button>
            </>
          ) : operationError ? (
            <>
              <Button variant="outline" onClick={s.closePlan}>
                {t('关闭', 'Close')}
              </Button>
              <Button onClick={() => void s.reviewAgain()}>{t('重新审阅', 'Review again')}</Button>
            </>
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
                ) : changedCount > 0 ? (
                  <Trash2 />
                ) : (
                  <Check />
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
