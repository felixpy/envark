import { useEffect, useRef } from 'react'
import { cleanupAvailability, cleanupEnvironmentTab } from '@/lib/cache-cleanup'
import { displayPath } from '@/lib/paths'
import { Database, RefreshCw, Trash2 } from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes, metadata } from '@/domain'
import { EcoDot, Empty, PageHeader } from '@/components/shared'
import { ActionButton, SelectionCheckbox } from '@/components/action-controls'
import { useSelection } from '@/hooks/use-selection'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip'
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
  const checked = useRef(false)
  useEffect(() => {
    if (!s.api.native || !s.loaded || s.busy || checked.current) return
    checked.current = true
    void s.recheckCaches()
  }, [s.api.native, s.loaded, s.busy])
  const caches = s.data.inventory.caches
  const selection = useSelection(
    caches.filter((c) => c.canClean).map((c) => c.id),
    s.completedSelectionIds,
  )
  const chosen = caches.filter((c) => selection.chosen.includes(c.id))
  const busyReason = s.busy
    ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
    : null
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('全局缓存', 'Global caches')}
        actions={
          <>
            <ActionButton
              variant="outline"
              reason={busyReason}
              onClick={() => void s.recheckCaches()}
            >
              {t('检查清理条件', 'Check cleanup availability')}
            </ActionButton>
            <ActionButton variant="outline" reason={busyReason} onClick={() => void s.refresh()}>
              <RefreshCw />
              {t('刷新缓存', 'Refresh caches')}
            </ActionButton>
          </>
        }
        description={t(
          '在此清理共享缓存，自动调用对应工具或移至回收站。再次使用时可能需要重新下载。',
          'Clean shared caches here using their native tools or the Trash. Future builds may need to download dependencies again.',
        )}
      />
      <div className="flex items-center gap-3 rounded-xl border p-4">
        <Database className="size-5 text-muted-foreground" />
        <div>
          <p className="text-xl font-semibold tabular-nums">
            {caches.some((cache) => !cache.size.complete) && '≈ '}
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
      {[true, false].map((canClean) => {
        const group = caches.filter((cache) => cache.canClean === canClean)
        if (!group.length) return null
        return (
          <section key={String(canClean)} className="space-y-3">
            <h2 className="text-sm font-medium">
              {canClean
                ? t('可清理', 'Available for cleanup')
                : t('暂不可清理', 'Cleanup unavailable')}
              <span className="ml-2 font-normal text-muted-foreground">
                {group.length} ·{' '}
                {formatBytes(group.reduce((sum, cache) => sum + cache.size.bytes, 0))}
              </span>
            </h2>
            {!canClean && (
              <p className="text-xs text-muted-foreground">
                {t(
                  '以下目录暂未满足清理条件，原因和可用操作见下方。',
                  'These directories are not ready for cleanup. See the reasons and available actions below.',
                )}
              </p>
            )}
            <Card className="overflow-hidden py-0 shadow-none">
              <CardContent className="px-0">
                <Table>
                  <TableHeader>
                    <TableRow>
                      <TableHead className="w-10 pl-4">
                        {canClean && (
                          <SelectionCheckbox
                            aria-label={t('选择全部可清理缓存', 'Select all eligible caches')}
                            reason={
                              busyReason ||
                              (!selection.eligibleCount
                                ? t('当前没有可清理缓存。', 'No eligible caches.')
                                : null)
                            }
                            checked={
                              selection.checked
                                ? true
                                : selection.chosen.length
                                  ? 'indeterminate'
                                  : false
                            }
                            onCheckedChange={(on) => selection.toggleAll(on === true)}
                          />
                        )}
                      </TableHead>
                      <TableHead>{t('缓存', 'Cache')}</TableHead>
                      <TableHead>{t('生态', 'Ecosystem')}</TableHead>
                      <TableHead>{t('清理方式', 'Cleanup strategy')}</TableHead>
                      <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
                    </TableRow>
                  </TableHeader>
                  <TableBody>
                    {group.map((cache) => (
                      <TableRow key={cache.id}>
                        <TableCell className="pl-4">
                          {canClean && (
                            <SelectionCheckbox
                              checked={cache.canClean && selection.selected.has(cache.id)}
                              reason={
                                busyReason ||
                                (!cache.canClean
                                  ? cache.warning ||
                                    t(
                                      '此目录尚无可用的清理策略。',
                                      'No cleanup strategy is available for this directory.',
                                    )
                                  : null)
                              }
                              aria-label={`${t('选择', 'Select')} ${cache.name}`}
                              onCheckedChange={(checked) =>
                                selection.toggle([cache.id], checked === true)
                              }
                            />
                          )}
                        </TableCell>
                        <TableCell>
                          <div className="font-medium">{cache.name}</div>
                          <p
                            className="mt-1 max-w-md truncate font-mono text-xs text-muted-foreground"
                            title={displayPath(cache.path)}
                          >
                            {displayPath(cache.path)}
                          </p>
                          {!cache.canClean && cleanupAvailability(cache, t) && (
                            <p
                              className="mt-2 max-w-md text-xs text-muted-foreground"
                              title={cache.cleanupIssue?.detail}
                            >
                              {cleanupAvailability(cache, t)}
                            </p>
                          )}
                        </TableCell>
                        <TableCell>
                          <span className="flex items-center gap-2 text-sm">
                            <EcoDot id={cache.provider} />
                            {metadata[cache.provider].short}
                          </span>
                        </TableCell>
                        <TableCell>
                          <Tooltip delayDuration={350}>
                            <TooltipTrigger asChild>
                              <Badge
                                variant={cache.canClean ? 'secondary' : 'outline'}
                                className="font-normal"
                                tabIndex={0}
                              >
                                {cache.canClean
                                  ? cache.strategy.startsWith('cargo-')
                                    ? s.data.settings.useTrash
                                      ? t('移至回收站', 'Move to Trash')
                                      : t('删除下载缓存', 'Remove downloaded cache')
                                    : cache.strategy.startsWith('gradle-')
                                      ? t('整理过期缓存', 'Prune expired caches')
                                      : t('工具原生清理', 'Native cleanup')
                                  : t('暂不可清理', 'Cleanup unavailable')}
                              </Badge>
                            </TooltipTrigger>
                            <TooltipContent
                              sideOffset={6}
                              className="max-w-72 text-left leading-relaxed"
                            >
                              {cache.strategy === 'maven-repository'
                                ? t(
                                    '可能包含无法重新下载的本地发布产物，暂不执行整库清理。',
                                    'May contain locally published artifacts that cannot be downloaded again; whole-repository cleanup is disabled.',
                                  )
                                : cache.strategy.startsWith('gradle-')
                                  ? t(
                                      '同时整理 Gradle 缓存和发行包，按保留策略保留仍在使用或近期使用的内容，不会全部清空。',
                                      'Prunes Gradle caches and distributions together, retaining entries required by its retention policy. This does not empty every cache.',
                                    )
                                  : cache.strategy.startsWith('cargo-')
                                    ? t(
                                        '取得 Cargo 缓存锁后清理，保留已安装工具与配置；有本地修改的 Git 缓存不会删除。',
                                        'Cleans downloaded dependencies while holding Cargo cache locks; installed tools, configuration, and modified Git checkouts are preserved.',
                                      )
                                    : !cache.canClean &&
                                        ['npm-verify', 'pnpm-prune', 'yarn-clean'].includes(
                                          cache.strategy,
                                        )
                                      ? t(
                                          '已找到缓存，但尚未确认对应的清理工具。管理 Node 环境后，点击刷新缓存重新检查。',
                                          'Cache found, but its cleanup tool is not confirmed. Manage the Node environment, then refresh caches to check again.',
                                        )
                                      : cache.warning || cache.strategy}
                            </TooltipContent>
                          </Tooltip>
                          {!cache.canClean && cache.cleanupIssue && (
                            <div className="mt-2 flex flex-wrap gap-1">
                              {['toolUnavailable', 'unsupportedTool', 'unavailable'].includes(
                                cache.cleanupIssue.reason,
                              ) && (
                                <ActionButton
                                  variant="link"
                                  size="sm"
                                  onClick={() =>
                                    s.go('env', cache.provider, {
                                      tab: cleanupEnvironmentTab(cache),
                                    })
                                  }
                                >
                                  {t('管理清理工具', 'Manage cleanup tool')}
                                </ActionButton>
                              )}
                              {!['unsafePath', 'localChanges'].includes(
                                cache.cleanupIssue.reason,
                              ) && (
                                <ActionButton
                                  variant="outline"
                                  size="sm"
                                  reason={busyReason}
                                  onClick={() =>
                                    void (cache.cleanupIssue?.reason === 'changed'
                                      ? s.refresh()
                                      : s.recheckCaches())
                                  }
                                >
                                  {t('重新检查', 'Check again')}
                                </ActionButton>
                              )}
                            </div>
                          )}
                        </TableCell>
                        <TableCell
                          className="text-right font-mono text-xs"
                          title={
                            !cache.size.complete
                              ? t(
                                  '大小尚未确认，可能是上次测量值。请刷新。',
                                  'Size is unconfirmed and may be a previous measurement. Refresh to confirm.',
                                )
                              : undefined
                          }
                        >
                          {!cache.size.complete && '≈ '}
                          {formatBytes(cache.size.bytes)}
                        </TableCell>
                      </TableRow>
                    ))}
                  </TableBody>
                </Table>
              </CardContent>
            </Card>
          </section>
        )
      })}
      {!caches.length && (
        <Empty>
          {t(
            '点击刷新缓存，检查共享缓存目录。',
            'Refresh caches to discover shared cache directories.',
          )}
        </Empty>
      )}
      <p className="text-xs text-muted-foreground">
        {t(
          '缓存总量是审阅上限，实际清理量由工具决定。硬链接和共享文件可能让逻辑大小大于物理占用。',
          'Cache totals are an upper bound. Native tools determine what can be removed. Hard links and shared files may make logical size larger than physical usage.',
        )}
      </p>
    </div>
  )
}
