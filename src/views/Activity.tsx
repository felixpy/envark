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
    `${item.title} ${item.detail}`.toLowerCase().includes(query.trim().toLowerCase()),
  )
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('操作记录', 'Activity')}
        description={t(
          '查看执行结果、失败原因与已确认释放的空间。记录仅保存在本机。',
          'Review results, failures, and verified reclaimed space. Activity stays on this computer.',
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
        <div className="space-y-2 rounded-xl border p-4">
          <h2 className="text-sm font-medium">{t('扫描提示', 'Scan notices')}</h2>
          {s.data.inventory.issues.map((issue, index) => (
            <p key={index} className="break-all text-xs text-muted-foreground">
              {issue}
            </p>
          ))}
        </div>
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
                {item.freedBytes > 0 && (
                  <span className="font-mono text-xs text-emerald-600">
                    −{formatBytes(item.freedBytes)}
                  </span>
                )}
                <Badge variant="outline">{item.status}</Badge>
              </summary>
              <pre className="mt-3 whitespace-pre-wrap break-all rounded-lg bg-muted p-3 font-mono text-xs text-muted-foreground">
                {item.detail}
              </pre>
            </details>
          ))}
          {!list.length && <Empty>{t('暂无操作记录。', 'No activity yet.')}</Empty>}
        </CardContent>
      </Card>
    </div>
  )
}
