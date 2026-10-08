import { displayPath, displayDiagnostic } from '@/lib/paths'
import { AlertTriangle, CheckCircle2, Trash2, XCircle } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes } from '@/domain'
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

export function OperationDialog() {
  const s = useStore()
  const { plan, result, t } = s
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
                ) : (
                  <XCircle className="size-4 shrink-0 text-destructive" />
                )}
                <div className="min-w-0 text-sm">
                  <div className="font-medium">{item.title}</div>
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
              {plan?.items.map((item, index) => (
                <div className="space-y-1 p-3 text-sm" key={index}>
                  <div className="flex justify-between gap-4 font-medium">
                    <span>{item.title}</span>
                    {item.bytes > 0 && <span className="font-mono">{formatBytes(item.bytes)}</span>}
                  </div>
                  {item.path && (
                    <p className="break-all font-mono text-xs text-muted-foreground">
                      {displayPath(item.path)}
                    </p>
                  )}
                  {item.command && (
                    <pre className="whitespace-pre-wrap break-all rounded bg-muted p-2 text-xs">
                      {item.command}
                    </pre>
                  )}
                  {item.restore && <p className="text-xs text-muted-foreground">{item.restore}</p>}
                </div>
              ))}
            </div>
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
              <Button onClick={() => void s.execute()}>
                <Trash2 />
                {t('确认执行', 'Confirm operation')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
