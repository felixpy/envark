import { CheckCircle2, ShieldCheck, XCircle } from 'lucide-react'
import { formatBytes, type OperationResult } from '@/domain'
import { useStore } from '@/store'
import { displayDiagnostic } from '@/lib/paths'

export function OperationResults({ result }: { result: OperationResult }) {
  const { t, plan } = useStore()
  const succeeded = result.items.filter((item) => item.status === 'success')
  const failed = result.items.filter((item) => item.status === 'failed')
  const cancelled = result.items.filter((item) => item.status === 'cancelled')
  const incomplete = [...failed, ...cancelled]
  const skipped = result.items.filter((item) => item.status === 'skipped')
  const list = (items: OperationResult['items']) => (
    <div className="mt-3 space-y-2">
      {items.map((item, index) => (
        <div key={index} className="flex gap-3 rounded-lg border p-3">
          {item.status === 'success' ? (
            <CheckCircle2 className="size-4 shrink-0 text-emerald-600" />
          ) : item.status === 'skipped' ? (
            <ShieldCheck className="size-4 shrink-0 text-muted-foreground" />
          ) : (
            <XCircle className="size-4 shrink-0 text-destructive" />
          )}
          <div className="min-w-0 text-sm">
            <p className="font-medium">{item.title}</p>
            {item.status === 'skipped' && <p>{t('已保留', 'Kept')}</p>}
            <pre className="mt-1 whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
              {displayDiagnostic(item.message)}
            </pre>
          </div>
        </div>
      ))}
    </div>
  )
  return (
    <div className="space-y-4">
      <div className="space-y-2 rounded-lg bg-muted p-4">
        <p className="font-medium">
          {t('完成', 'Completed')} {succeeded.length} · {t('保留', 'Kept')} {skipped.length} ·{' '}
          {t('失败', 'Failed')} {failed.length}
          {cancelled.length > 0 && (
            <>
              {' '}
              · {t('中断', 'Interrupted')} {cancelled.length}
            </>
          )}
        </p>
        {plan && (['clean', 'removeWorktree'].includes(plan.kind) || result.removedBytes > 0) && (
          <>
            <p className="text-sm">
              {t('已移除的逻辑大小', 'Logical size removed')}:{' '}
              <span className="font-mono text-emerald-600">{formatBytes(result.removedBytes)}</span>
            </p>
            {plan.kind === 'clean' && plan.useTrash && (
              <p className="text-xs text-muted-foreground">
                {t(
                  '移入回收站的文件仍占用磁盘。侧栏显示系统报告的实际使用量。',
                  'Files in Trash still occupy disk space. The sidebar shows actual usage reported by the system.',
                )}
              </p>
            )}
          </>
        )}
      </div>
      {result.cancelled && (
        <p className="text-sm text-amber-600">
          {t(
            '已取消，已完成的操作不会回滚。',
            'Cancelled. Completed operations are not rolled back.',
          )}
        </p>
      )}
      {incomplete.length > 0 && (
        <section aria-label={t('失败项', 'Failed items')}>
          <h3 className="text-sm font-medium text-destructive">
            {t('以下项目未完成，请查看原因', 'These items did not finish; review their details')}
          </h3>
          {list(incomplete)}
        </section>
      )}
      {skipped.length > 0 && (
        <details>
          <summary className="cursor-pointer text-sm">
            {t('查看保留项', 'View kept items')} ({skipped.length})
          </summary>
          {list(skipped)}
        </details>
      )}
      {succeeded.length > 0 && (
        <details>
          <summary className="cursor-pointer text-sm">
            {t('查看已完成项', 'View completed items')} ({succeeded.length})
          </summary>
          {list(succeeded)}
        </details>
      )}
    </div>
  )
}
