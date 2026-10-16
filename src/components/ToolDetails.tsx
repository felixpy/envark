import { Check, RotateCw } from 'lucide-react'
import { Popover } from 'radix-ui'
import type { ReactNode } from 'react'
import { type ProviderId, type Tool, updateKind } from '@/domain'
import { updateIssueLabel } from '@/lib/tool-update-info'
import { useStore } from '@/store'
import { Button } from './ui/button'

function Details({
  trigger,
  label,
  children,
}: {
  trigger: ReactNode
  label: string
  children: ReactNode
}) {
  return (
    <Popover.Root>
      <Popover.Trigger asChild>
        <button
          type="button"
          aria-label={label}
          className="rounded-sm text-left underline-offset-4 hover:underline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-ring"
        >
          {trigger}
        </button>
      </Popover.Trigger>
      <Popover.Portal>
        <Popover.Content
          sideOffset={8}
          align="start"
          aria-label={label}
          className="z-50 w-80 max-w-[calc(100vw-32px)] space-y-3 rounded-lg border bg-popover p-4 text-sm text-popover-foreground shadow-md outline-none"
        >
          {children}
        </Popover.Content>
      </Popover.Portal>
    </Popover.Root>
  )
}

export function ToolVersion({ item, provider }: { item: Tool; provider: ProviderId }) {
  const s = useStore()
  const { t } = s
  const kind = updateKind(item)
  const issue = updateIssueLabel(item, t)
  return (
    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs">
      <span className="font-mono">{item.version}</span>
      {['major', 'minor'].includes(kind) && (
        <span className="font-mono text-emerald-600">→ {item.latest}</span>
      )}
      {kind === 'latest' && !issue && (
        <span title={t('已是最新版本', 'Up to date')}>
          <Check className="size-3.5 text-muted-foreground" />
          <span className="sr-only">{t('已是最新版本', 'Up to date')}</span>
        </span>
      )}
      {kind === 'ahead' && !issue && (
        <span className="text-muted-foreground">
          {t('高于已发布版本', 'Ahead of published version')}
        </span>
      )}
      {issue && item.updateIssue?.reason === 'runtimeManaged' && (
        <span className="text-muted-foreground">{issue}</span>
      )}
      {issue && item.updateIssue?.reason !== 'runtimeManaged' && (
        <Details
          trigger={<span className="text-amber-700 dark:text-amber-400">{issue}</span>}
          label={`${t('查看更新详情', 'View update details')} ${item.name}`}
        >
          <p className="font-medium">
            {item.name} · {issue}
          </p>
          <pre className="max-h-40 overflow-auto whitespace-pre-wrap break-all font-mono text-xs leading-relaxed text-muted-foreground">
            {item.updateIssue?.detail}
          </pre>
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="outline"
              disabled={s.updateChecks[provider] === 'checking'}
              onClick={() => void s.checkToolUpdates(provider)}
            >
              <RotateCw />
              {t('重新查询', 'Retry lookup')}
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => s.go('env', provider, { tab: 'config' })}
            >
              {t('查看配置', 'View configuration')}
            </Button>
          </div>
        </Details>
      )}
      {kind === 'unknown' && !issue && (
        <span className="text-muted-foreground">
          {s.updateChecks[provider] === 'checking'
            ? t('正在查询…', 'Checking…')
            : t('尚未查询', 'Not checked')}
        </span>
      )}
    </div>
  )
}

function sourceLabel(value: string) {
  try {
    return new URL(value.replace(/^(registry\+|sparse\+)/, '')).host
  } catch {
    return value
  }
}

export function ToolSource({ item, manager }: { item: Tool; manager: string }) {
  const { t } = useStore()
  const origin = item.installation?.origin
  const current = item.updateSource?.label
  const evidence = item.installation?.evidence
  const originLabel =
    evidence === 'pnpm environment'
      ? t('环境源配置记录', 'Recorded environment source configuration')
      : evidence === 'Yarn artifact' || evidence === 'legacy manifest'
        ? t('安装下载记录', 'Recorded installation download')
        : evidence === 'uv receipt'
          ? t('安装时保存的源配置', 'Source configuration saved at installation')
          : t('安装来源记录', 'Recorded installation source')
  return (
    <div className="space-y-1 text-xs text-muted-foreground">
      <p>
        {manager}
        {item.runtime && (
          <>
            {' '}
            · <span>{item.runtime}</span>
          </>
        )}
      </p>
      {(current || item.installation) && (
        <Details
          trigger={current ? sourceLabel(current) : t('来源详情', 'Source details')}
          label={`${t('查看来源', 'View source')} ${item.name}`}
        >
          <p className="font-medium text-foreground">{item.name}</p>
          <dl className="space-y-2 break-all text-xs">
            <div>
              <dt className="text-muted-foreground">{t('当前查询源', 'Current lookup source')}</dt>
              <dd>{current || t('尚未确认', 'Not confirmed')}</dd>
            </div>
            <div>
              <dt className="text-muted-foreground">{originLabel}</dt>
              <dd>
                {origin ||
                  t(
                    '安装时未记录，当前配置不能证明历史来源。',
                    'Not recorded. Current configuration does not establish historical origin.',
                  )}
              </dd>
              {origin &&
                (evidence === 'pnpm environment' ||
                  evidence === 'Yarn artifact' ||
                  evidence === 'legacy manifest') && (
                  <p className="mt-1 text-muted-foreground">
                    {t(
                      '此记录不能单独证明包的历史仓库来源。',
                      'This record alone does not establish the package’s historical registry.',
                    )}
                  </p>
                )}
            </div>
            {item.installation?.evidence && (
              <div>
                <dt className="text-muted-foreground">{t('记录类型', 'Evidence')}</dt>
                <dd>{item.installation.evidence}</dd>
              </div>
            )}
            {item.installation?.entry !== item.installation?.package && (
              <div>
                <dt className="text-muted-foreground">{t('安装别名', 'Installation alias')}</dt>
                <dd>{item.installation?.entry}</dd>
              </div>
            )}
          </dl>
        </Details>
      )}
    </div>
  )
}
