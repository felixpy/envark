import { useState } from 'react'
import { Database, Trash2 } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes, metadata } from '@/domain'
import { EcoDot, Empty, PageHeader } from '@/components/shared'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Checkbox } from '@/components/ui/checkbox'
import { Badge } from '@/components/ui/badge'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export default function Caches() {
  const s = useStore()
  const { t } = s
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const caches = s.data.inventory.caches
  const chosen = caches.filter((c) => selected.has(c.id) && c.canClean)
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('全局缓存', 'Global caches')}
        description={t(
          '共享缓存由原工具负责清理。清理后再次使用时可能需要重新下载。',
          'Shared caches are cleaned by their owning tools. Future builds may need to download dependencies again.',
        )}
      />
      <div className="flex items-center gap-3 rounded-xl border p-4">
        <Database className="size-5 text-muted-foreground" />
        <div>
          <p className="text-xl font-semibold tabular-nums">
            {formatBytes(caches.reduce((sum, cache) => sum + cache.size.bytes, 0))}
          </p>
          <p className="text-xs text-muted-foreground">
            {caches.length}{' '}
            {t('个已识别缓存目录 · 逻辑大小', 'known cache directories · logical size')}
          </p>
        </div>
        <Button
          className="ml-auto"
          disabled={!chosen.length || s.busy}
          onClick={() => void s.prepare({ kind: 'cleanCaches', ids: chosen.map((c) => c.id) })}
        >
          <Trash2 />
          {t('审阅清理', 'Review cleanup')}
          {chosen.length > 0 && ` (${chosen.length})`}
        </Button>
      </div>
      <Card className="overflow-hidden py-0 shadow-none">
        <CardContent className="px-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10" />
                <TableHead>{t('缓存', 'Cache')}</TableHead>
                <TableHead>{t('生态', 'Ecosystem')}</TableHead>
                <TableHead>{t('清理方式', 'Cleanup strategy')}</TableHead>
                <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {caches.map((cache) => (
                <TableRow key={cache.id}>
                  <TableCell className="pl-4">
                    <Checkbox
                      checked={selected.has(cache.id)}
                      disabled={!cache.canClean}
                      aria-label={`${t('选择', 'Select')} ${cache.name}`}
                      onCheckedChange={(checked) =>
                        setSelected((previous) => {
                          const next = new Set(previous)
                          if (checked) next.add(cache.id)
                          else next.delete(cache.id)
                          return next
                        })
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <div className="font-medium">{cache.name}</div>
                    <p
                      className="mt-1 max-w-md truncate font-mono text-xs text-muted-foreground"
                      title={cache.path}
                    >
                      {cache.path}
                    </p>
                  </TableCell>
                  <TableCell>
                    <span className="flex items-center gap-2 text-sm">
                      <EcoDot id={cache.provider} />
                      {metadata[cache.provider].short}
                    </span>
                  </TableCell>
                  <TableCell>
                    {cache.canClean ? (
                      <Badge variant="secondary" className="font-normal">
                        {t('工具原生清理', 'Native cleanup')}
                      </Badge>
                    ) : (
                      <Badge variant="outline" className="font-normal">
                        {t('由原工具管理', 'Owner-managed')}
                      </Badge>
                    )}
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {!cache.size.complete && '≥ '}
                    {formatBytes(cache.size.bytes)}
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {!caches.length && (
            <Empty>
              {t('扫描后显示已识别的缓存目录。', 'Known cache directories appear after a scan.')}
            </Empty>
          )}
        </CardContent>
      </Card>
      <p className="text-xs text-muted-foreground">
        {t(
          '缓存总量是审阅上限，实际清理量由工具决定。硬链接和共享文件可能让逻辑大小大于物理占用。',
          'Cache totals are an upper bound. Native tools determine what can be removed. Hard links and shared files may make logical size larger than physical usage.',
        )}
      </p>
    </div>
  )
}
