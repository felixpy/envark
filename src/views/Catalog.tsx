import { ArrowRight, Cpu, FileCog, HardDriveDownload, Package, Plus, Wrench } from 'lucide-react'
import { categories, emptyProvider, metadata, providerIds, type ProviderId } from '@/domain'
import { useStore } from '@/store'
import { EcoDot, PageHeader } from '@/components/shared'
import { Badge } from '@/components/ui/badge'
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'

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
  return (
    <div className="space-y-8">
      <PageHeader
        title={t('环境与工具', 'Environments & tools')}
        description={t(
          '按类别管理开发环境。每个提供方展示它具备的能力：运行时、全局工具、下载资源与配置。',
          'Manage environments by category, with the capabilities each provider supports.',
        )}
      />
      {categories.map((category) => (
        <section key={category.id} className="space-y-3">
          <h2 className="text-base font-semibold">{t(category.zh, category.en)}</h2>
          <div className="grid grid-cols-3 gap-4 max-xl:grid-cols-2 max-md:grid-cols-1">
            {providerIds
              .filter((id) => metadata[id].category === category.id)
              .map((id) => {
                const p = s.data.inventory.providers.find((p) => p.id === id) ?? emptyProvider(id)
                const meta = metadata[id]
                return (
                  <Card
                    key={id}
                    className="cursor-pointer gap-4 py-4 shadow-none transition-colors hover:border-foreground/30"
                    role="button"
                    tabIndex={0}
                    onClick={() => s.go('env', id)}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter' || e.key === ' ') {
                        e.preventDefault()
                        s.go('env', id)
                      }
                    }}
                  >
                    <CardHeader className="px-4">
                      <CardTitle className="flex items-center gap-2">
                        <EcoDot id={id} />
                        {meta.name}
                      </CardTitle>
                      <CardDescription className="min-h-10">
                        {t(meta.description[0], meta.description[1])}
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="flex flex-wrap gap-1 px-4">
                      {capabilities(id).map((cap) => (
                        <Badge key={cap.id} variant="secondary" className="gap-1 font-normal">
                          <cap.icon className="size-3" />
                          {t(cap.zh, cap.en)}
                        </Badge>
                      ))}
                    </CardContent>
                    <CardFooter className="justify-between gap-2 border-t px-4 pt-3 text-xs text-muted-foreground">
                      <span>
                        {p.runtimes.find((r) => r.active)?.version ??
                          (p.detected ? t('已检测到', 'Detected') : t('未检测到', 'Not detected'))}
                      </span>
                      <ArrowRight className="size-3.5" />
                    </CardFooter>
                  </Card>
                )
              })}
          </div>
        </section>
      ))}
      <div className="flex items-center gap-3 rounded-xl border border-dashed p-4 text-sm text-muted-foreground">
        <Plus className="size-4" />
        {t(
          '以统一的能力结构接入更多工具与生态。',
          'One capability model, with room for more tools and ecosystems.',
        )}
      </div>
    </div>
  )
}
