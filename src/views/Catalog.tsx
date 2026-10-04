import { ArrowRight, Cpu, FileCog, HardDriveDownload, Package, Wrench } from 'lucide-react'
import {
  categories,
  emptyProvider,
  metadata,
  providerIds,
  updateKind,
  type ProviderId,
} from '@/domain'
import { useStore } from '@/store'
import { EcoDot, PageHeader } from '@/components/shared'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Card, CardDescription, CardFooter, CardHeader, CardTitle } from '@/components/ui/card'

export function capabilities(id: ProviderId) {
  const language = metadata[id].category === 'lang'
  return [
    ...(language || id === 'ollama'
      ? [{ id: 'runtime', zh: '运行时', en: 'Runtimes', icon: Cpu }]
      : []),
    ...(language
      ? [
          { id: 'pm', zh: '包管理器', en: 'Package managers', icon: Package },
          { id: 'global', zh: '全局工具', en: 'Global tools', icon: Wrench },
        ]
      : []),
    ...(!language
      ? [
          {
            id: 'assets',
            zh: id === 'ollama' ? '模型' : '浏览器',
            en: id === 'ollama' ? 'Models' : 'Browsers',
            icon: HardDriveDownload,
          },
        ]
      : []),
    { id: 'config', zh: '配置文件', en: 'Configuration', icon: FileCog },
  ]
}

export default function Catalog() {
  const s = useStore()
  const { t } = s
  const filter = s.focus.filter
  const providers = providerIds.filter((id) => {
    const p = s.data.inventory.providers.find((p) => p.id === id) ?? emptyProvider(id)
    if (filter === 'runtimes') return p.runtimes.length > 0
    if (filter === 'downloads') return p.assets.length > 0
    if (filter === 'updates')
      return [...p.tools, ...p.packageManagers].some((tool) =>
        ['major', 'minor'].includes(updateKind(tool)),
      )
    return true
  })
  return (
    <div className="space-y-8">
      <PageHeader
        title={t('环境与工具', 'Environments & tools')}
        description={t(
          '查看已安装的环境、工具和下载资源。',
          'Browse installed environments, tools, and downloads.',
        )}
        actions={
          filter ? (
            <Button variant="outline" onClick={() => s.go('env')}>
              {t('显示全部环境', 'Show all environments')}
            </Button>
          ) : undefined
        }
      />
      {filter && (
        <p role="status" className="text-sm text-muted-foreground">
          {filter === 'updates'
            ? t('仅显示有可用更新的环境', 'Showing environments with available updates')
            : filter === 'runtimes'
              ? t('仅显示已安装运行时的环境', 'Showing environments with installed runtimes')
              : t('仅显示有下载资源的环境', 'Showing environments with downloads')}
        </p>
      )}
      {!providers.length && (
        <p className="py-8 text-sm text-muted-foreground">
          {t('没有符合条件的环境。', 'No matching environments.')}
        </p>
      )}
      {categories
        .filter((category) => providers.some((id) => metadata[id].category === category.id))
        .map((category) => (
          <section key={category.id} className="space-y-3">
            <h2 className="text-base font-semibold">{t(category.zh, category.en)}</h2>
            <div className="grid grid-cols-3 gap-4 max-xl:grid-cols-2 max-md:grid-cols-1">
              {providers
                .filter((id) => metadata[id].category === category.id)
                .map((id) => {
                  const p = s.data.inventory.providers.find((p) => p.id === id) ?? emptyProvider(id)
                  const meta = metadata[id]
                  const open = () =>
                    s.go('env', id, {
                      filter,
                      tab:
                        filter === 'downloads'
                          ? 'assets'
                          : filter === 'updates'
                            ? p.tools.some((tool) => ['major', 'minor'].includes(updateKind(tool)))
                              ? 'global'
                              : 'pm'
                            : undefined,
                    })
                  return (
                    <Card
                      key={id}
                      className={`cursor-pointer gap-4 py-4 shadow-none transition-colors hover:border-foreground/40 focus-visible:outline-2 focus-visible:outline-ring ${!p.detected ? 'border-dashed bg-muted/50' : ''}`}
                      role="button"
                      tabIndex={0}
                      onClick={open}
                      onKeyDown={(e) => {
                        if (e.key === 'Enter' || e.key === ' ') {
                          e.preventDefault()
                          open()
                        }
                      }}
                    >
                      <CardHeader className="px-4">
                        <CardTitle className="flex items-center gap-2">
                          <EcoDot id={id} className={!p.detected ? 'grayscale opacity-60' : ''} />
                          {meta.name}
                        </CardTitle>
                        <CardDescription className="min-h-10">
                          {t(meta.description[0], meta.description[1])}
                        </CardDescription>
                      </CardHeader>
                      <CardFooter className="justify-between gap-2 border-t px-4 pt-3 text-xs text-muted-foreground">
                        <Badge
                          variant={p.detected ? 'secondary' : 'outline'}
                          className={
                            !p.detected
                              ? 'border-muted-foreground/40 bg-background text-muted-foreground'
                              : ''
                          }
                        >
                          {p.detected
                            ? t('已检测到', 'Detected')
                            : s.data.inventory.scannedAt
                              ? t('未检测到', 'Not detected')
                              : t('待扫描', 'Not scanned')}
                        </Badge>
                        <span className="ml-auto font-mono">
                          {p.runtimes.find((r) => r.active)?.version}
                        </span>
                        <ArrowRight className="size-3.5" />
                      </CardFooter>
                    </Card>
                  )
                })}
            </div>
          </section>
        ))}
    </div>
  )
}
