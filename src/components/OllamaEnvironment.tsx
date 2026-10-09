import { ArrowUp, Box, Play, RefreshCw, Square, Trash2 } from 'lucide-react'
import type { Provider } from '@/domain'
import { updateKind } from '@/domain'
import { toolUpdateReason } from '@/action-availability'
import { displayPath } from '@/lib/paths'
import { useStore } from '@/store'
import { ActionButton as Button } from './action-controls'
import { ManagerInstallDialog } from './ManagerInstallDialog'
import { Badge } from './ui/badge'
import { Card, CardAction, CardContent, CardDescription, CardHeader, CardTitle } from './ui/card'

export function OllamaService({
  provider,
  embedded = false,
}: {
  provider: Provider
  embedded?: boolean
}) {
  const s = useStore()
  const { t } = s
  const service = provider.service
  const canStart = provider.tools.some((tool) => tool.name === 'ollama' && tool.path)
  const owned = service?.owned ?? false
  const running = service?.running ?? false
  return (
    <section
      aria-label={t('本地服务', 'Local service')}
      className={`${embedded ? 'border-t' : 'rounded-xl border'} flex flex-wrap items-center justify-between gap-4 bg-muted/25 px-6 py-4`}
    >
      <div className="min-w-0 space-y-1.5">
        <div className="flex flex-wrap items-center gap-2 text-sm">
          <span className="font-medium">{t('本地服务', 'Local service')}</span>
          <span
            className="inline-flex items-center gap-1.5 text-xs text-muted-foreground"
            role="status"
          >
            <span
              className={`size-1.5 rounded-full ${running ? 'bg-emerald-500' : owned ? 'bg-amber-500' : 'bg-muted-foreground/50'}`}
            />
            {running
              ? t('服务运行中', 'Service running')
              : owned
                ? t('服务无响应', 'Service unresponsive')
                : t('服务已停止', 'Service stopped')}
          </span>
          {running && !owned && (
            <Badge variant="outline" className="font-normal">
              {t('外部服务', 'External service')}
            </Badge>
          )}
        </div>
        <p className="text-xs text-muted-foreground">
          {running ? (
            <span className="font-mono">{service?.endpoint}</span>
          ) : owned ? (
            t('先停止当前进程，再重新启动。', 'Stop the current process before starting it again.')
          ) : !canStart ? (
            t('先安装 Ollama，再启动本地服务。', 'Install Ollama to start a local service.')
          ) : (
            t(
              '启动后即可下载和管理本地模型。',
              'Start the service to download and manage local models.',
            )
          )}
        </p>
      </div>
      {!owned && !running && !canStart && <ManagerInstallDialog provider="ollama" prominent />}
      {(owned || (!running && canStart)) && (
        <Button
          size="sm"
          variant={owned ? 'outline' : 'default'}
          reason={
            s.busy ? t('请等待当前操作完成。', 'Wait for the current operation to finish.') : null
          }
          onClick={() =>
            void s.prepare({
              kind: 'serviceAction',
              provider: 'ollama',
              action: owned ? 'stop' : 'start',
            })
          }
        >
          {owned ? <Square /> : <Play />}
          {owned ? t('停止服务', 'Stop service') : t('启动服务', 'Start service')}
        </Button>
      )}
    </section>
  )
}

export function OllamaProgram({ provider }: { provider: Provider }) {
  const s = useStore()
  const { t } = s
  const program = provider.tools.find((tool) => tool.name === 'ollama')
  const busyReason = s.busy
    ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
    : null
  if (!program) {
    return (
      <Card className="gap-0 overflow-hidden shadow-none">
        <div className="flex flex-wrap items-center justify-between gap-6 p-6">
          <div className="flex items-start gap-4">
            <div className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-muted">
              <Box className="size-5 text-muted-foreground" />
            </div>
            <div className="space-y-1.5">
              <h2 className="font-semibold">
                {t('安装 Ollama 程序', 'Install the Ollama application')}
              </h2>
              <p className="max-w-lg text-sm text-muted-foreground">
                {t(
                  '安装程序后，在这里启动服务、下载模型和管理更新。',
                  'Install the application to start its service, download models, and manage updates here.',
                )}
              </p>
            </div>
          </div>
          <ManagerInstallDialog provider="ollama" prominent />
        </div>
        {(provider.service?.running || provider.service?.owned) && (
          <OllamaService provider={provider} embedded />
        )}
      </Card>
    )
  }
  const newer = ['major', 'minor'].includes(updateKind(program))
  const source =
    program.source === 'ollama-official'
      ? t('官方发行包', 'Official release')
      : program.source === 'ollama-homebrew'
        ? 'Homebrew'
        : t('已有安装', 'Existing installation')
  return (
    <Card className="gap-0 overflow-hidden pb-0 shadow-none">
      <CardHeader className="pb-5">
        <CardTitle>{t('Ollama 程序', 'Ollama application')}</CardTitle>
        <CardDescription>
          {t('程序更新与模型数据独立管理。', 'Application updates keep your downloaded models.')}
        </CardDescription>
        <CardAction>
          <Button
            size="sm"
            variant="ghost"
            reason={busyReason}
            onClick={() =>
              s.data.settings.checkUpdates ? void s.checkToolUpdates('ollama') : s.go('settings')
            }
          >
            <RefreshCw />
            {s.data.settings.checkUpdates
              ? t('检查更新', 'Check for updates')
              : t('开启更新检查', 'Enable update checks')}
          </Button>
        </CardAction>
      </CardHeader>
      <CardContent className="space-y-4 pb-5">
        <div className="flex flex-wrap items-center justify-between gap-4">
          <div className="flex items-center gap-3">
            <div className="flex size-10 shrink-0 items-center justify-center rounded-lg bg-muted">
              <Box className="size-5 text-muted-foreground" />
            </div>
            <div className="space-y-1">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-sm font-medium">{program.version}</span>
                {newer && (
                  <Badge variant="secondary" className="font-normal">
                    {t('可更新至', 'Update to')} {program.latest}
                  </Badge>
                )}
              </div>
              <p className="text-xs text-muted-foreground">
                {source}
                {updateKind(program) === 'latest' ? ` · ${t('已是最新版本', 'Up to date')}` : ''}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            {newer && program.canUpdate && (
              <Button
                size="sm"
                variant="outline"
                reason={busyReason || toolUpdateReason(program, s.data.settings.checkUpdates, t)}
                onClick={() =>
                  void s.prepare({ kind: 'updateTool', provider: 'ollama', id: program.id })
                }
              >
                <ArrowUp />
                {t('更新', 'Update')}
              </Button>
            )}
            {program.canRemove && (
              <Button
                size="sm"
                variant="ghost"
                reason={busyReason}
                onClick={() =>
                  void s.prepare({ kind: 'removeTool', provider: 'ollama', id: program.id })
                }
              >
                <Trash2 />
                {t('卸载', 'Remove')}
              </Button>
            )}
          </div>
        </div>
        <details className="text-xs text-muted-foreground">
          <summary className="w-fit cursor-pointer rounded-sm outline-offset-4 hover:text-foreground focus-visible:outline-2">
            {t('安装信息', 'Installation details')}
          </summary>
          <div className="mt-2 space-y-2 rounded-lg bg-muted/40 p-3">
            {program.path && <p className="break-all font-mono">{displayPath(program.path)}</p>}
            {!program.canUpdate && (
              <p>
                {t(
                  '安装来源尚未验证。可以启动服务和管理模型，暂不能更改这份程序安装。',
                  'The installation source is unverified. Service and model actions are available; this program installation cannot be changed yet.',
                )}
              </p>
            )}
          </div>
        </details>
      </CardContent>
      <OllamaService provider={provider} embedded />
    </Card>
  )
}
