import { displayDiagnostic } from '@/lib/paths'
import { useState } from 'react'
import { CheckCircle2, Clock3, XCircle } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes } from '@/domain'
import { Empty, PageHeader, SearchInput } from '@/components/shared'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'

export default function Activity() {
  const s = useStore()
  const { t } = s
  const [query, setQuery] = useState('')
  const list = s.data.activity.filter((item) =>
    `${item.title} ${displayDiagnostic(item.detail)}`
      .toLowerCase()
      .includes(query.trim().toLowerCase()),
  )
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('操作记录', 'Activity')}
        description={t(
          '查看执行结果、失败原因与移除的逻辑大小。实际磁盘释放量未测量，记录仅保存在本机。',
          'Review results, failures, and logical size removed. Physical disk reclamation is unmeasured. Activity stays on this computer.',
        )}
        actions={
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t('搜索记录', 'Search activity')}
          />
        }
      />
      {s.data.inventory.issues.length > 0 && (
        <details className="space-y-2 rounded-xl border p-4">
          <summary className="cursor-pointer text-sm font-medium">
            {t('扫描提示', 'Scan notices')} ({s.data.inventory.issues.length})
          </summary>
          {s.data.inventory.issues.map((issue, index) => (
            <p key={index} className="break-all text-xs text-muted-foreground">
              {displayDiagnostic(issue)}
            </p>
          ))}
        </details>
      )}
      <Card className="py-0 shadow-none">
        <CardContent className="divide-y px-5">
          {list.map((item) => (
            <details key={item.id} className="py-4">
              <summary className="flex cursor-pointer list-none items-center gap-3">
                {item.status === 'success' ? (
                  <CheckCircle2 className="size-4 text-emerald-600" />
                ) : item.status === 'cancelled' ? (
                  <Clock3 className="size-4 text-muted-foreground" />
                ) : (
                  <XCircle className="size-4 text-destructive" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="text-sm font-medium">{item.title}</p>
                  <p className="text-xs text-muted-foreground">
                    {new Date(item.time * 1000).toLocaleString()}
                  </p>
                </div>
                {item.removedBytes > 0 && (
                  <span className="font-mono text-xs text-emerald-600">
                    {t('移除', 'Removed')} {formatBytes(item.removedBytes)}
                  </span>
                )}
                <Badge variant="outline">{item.status}</Badge>
              </summary>
              <pre className="mt-3 whitespace-pre-wrap break-all rounded-lg bg-muted p-3 font-mono text-xs text-muted-foreground">
                {displayDiagnostic(item.detail)}
              </pre>
            </details>
          ))}
          {!list.length && <Empty>{t('暂无操作记录。', 'No activity yet.')}</Empty>}
        </CardContent>
      </Card>
    </div>
  )
}
