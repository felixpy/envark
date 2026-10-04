import { Database, Trash2 } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes, metadata } from '@/domain'
import { EcoDot, Empty, PageHeader } from '@/components/shared'
import { ActionButton, SelectionCheckbox } from '@/components/action-controls'
import { useSelection } from '@/hooks/use-selection'
import { Card, CardContent } from '@/components/ui/card'
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
  const caches = s.data.inventory.caches
  const selection = useSelection(caches.filter((c) => c.canClean).map((c) => c.id))
  const chosen = caches.filter((c) => selection.chosen.includes(c.id))
  const busyReason = s.busy
    ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
    : null
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
        <ActionButton
          className="ml-auto"
          reason={
            busyReason ||
            (!chosen.length ? t('请先选择可清理的缓存。', 'Select an eligible cache first.') : null)
          }
          onClick={() => void s.prepare({ kind: 'cleanCaches', ids: chosen.map((c) => c.id) })}
        >
          <Trash2 />
          {t('审阅清理', 'Review cleanup')}
          {chosen.length > 0 && ` (${chosen.length})`}
        </ActionButton>
      </div>
      <p role="status" className="text-xs text-muted-foreground">
        {selection.eligibleCount}{' '}
        {t(
          '项缓存可选；不支持清理的缓存会显示原因。',
          'caches selectable. Unavailable caches explain why below.',
        )}
      </p>
      <Card className="overflow-hidden py-0 shadow-none">
        <CardContent className="px-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10 pl-4">
                  <SelectionCheckbox
                    aria-label={t('选择全部可清理缓存', 'Select all eligible caches')}
                    reason={
                      busyReason ||
                      (!selection.eligibleCount
                        ? t(
                            '当前没有可清理缓存，原因见各行说明。',
                            'No eligible caches. See the reasons in each row.',
                          )
                        : null)
                    }
                    checked={
                      selection.checked ? true : selection.chosen.length ? 'indeterminate' : false
                    }
                    onCheckedChange={(on) => selection.toggleAll(on === true)}
                  />
                </TableHead>
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
                    <SelectionCheckbox
                      checked={cache.canClean && selection.selected.has(cache.id)}
                      reason={
                        busyReason ||
                        (!cache.canClean
                          ? cache.warning ||
                            t(
                              '尚不支持此缓存的原生清理，请使用原工具管理。',
                              'Native cleanup is unavailable. Use the owning tool to manage this cache.',
                            )
                          : null)
                      }
                      aria-label={`${t('选择', 'Select')} ${cache.name}`}
                      onCheckedChange={(checked) => selection.toggle([cache.id], checked === true)}
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
                    <p className="mt-2 max-w-80 whitespace-normal text-xs text-muted-foreground">
                      {cache.warning ||
                        (!cache.canClean
                          ? t(
                              '尚不支持此缓存的原生清理，请使用原工具管理。',
                              'Native cleanup is unavailable. Use the owning tool to manage this cache.',
                            )
                          : cache.strategy)}
                    </p>
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
