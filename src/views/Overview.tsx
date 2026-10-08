import { displayDiagnostic } from '@/lib/paths'
import {
  Archive,
  ArrowRight,
  Database,
  FolderPlus,
  FolderGit2,
  GitFork,
  HardDriveDownload,
  RefreshCw,
  AlertTriangle,
} from 'lucide-react'
import { useStore } from '@/store'
import { formatBytes, idle, metadata, projectBytes, updateKind } from '@/domain'
import { EcoDot, Empty, PageHeader } from '@/components/shared'
import {
  Card,
  CardAction,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@/components/ui/card'
import { Button } from '@/components/ui/button'

export default function Overview() {
  const s = useStore()
  const { t } = s
  const { inventory, settings, activity } = s.data
  const inactive = inventory.projects.filter((p) => idle(p, settings.idleDays) && !p.protected)
  const idleSize = inactive.reduce((sum, p) => sum + projectBytes(p), 0)
  const cacheSize = inventory.caches.reduce((sum, c) => sum + c.size.bytes, 0)
  const updates = inventory.providers
    .flatMap((p) => [...p.tools, ...p.packageManagers])
    .filter((tool) => ['major', 'minor'].includes(updateKind(tool)))
  const stats = [
    {
      label: t('可审阅空间', 'Space to review'),
      value: formatBytes(idleSize + cacheSize),
      hint: t('闲置项目产物与已识别缓存', 'Inactive project artifacts and known caches'),
      action: () =>
        inactive.length
          ? s.go('projects', null, {
              filter: 'idle',
            })
          : s.go('caches'),
    },
    {
      label: t('运行时版本', 'Runtime versions'),
      value: inventory.providers.reduce((sum, p) => sum + p.runtimes.length, 0),
      hint: t('本机已检测到的版本', 'Installed versions detected on this computer'),
      action: () => s.go('env', null, { filter: 'runtimes' }),
    },
    {
      label: t('项目', 'Projects'),
      value: inventory.projects.filter((p) => !p.isWorktree).length,
      hint: `${inactive.filter((p) => !p.isWorktree).length} ${t('个闲置超过', 'inactive for over')} ${settings.idleDays} ${t('天', 'days')}`,
      action: () => s.go('projects'),
    },
    {
      label: t('可用更新', 'Available updates'),
      value: settings.checkUpdates ? updates.length : '—',
      hint: settings.checkUpdates
        ? t('包管理器与全局工具', 'Package managers and global tools')
        : t('联网检查尚未开启', 'Online checks are turned off'),
      action: () =>
        settings.checkUpdates ? s.go('env', null, { filter: 'updates' }) : s.go('settings'),
    },
  ]
  const tasks = [
    ...(inactive.length
      ? [
          {
            icon: Archive,
            title: `${inactive.length} ${t('个工作区长期未活动', 'inactive workspaces')}`,
            description: `${t('可重建产物占用', 'Regenerable artifacts use')} ${formatBytes(idleSize)}`,
            cta: t('审阅清理', 'Review cleanup'),
            action: () => s.go('projects', null, { filter: 'idle' }),
          },
        ]
      : []),
    ...(cacheSize
      ? [
          {
            icon: Database,
            title: `${t('全局缓存共', 'Global caches use')} ${formatBytes(cacheSize)}`,
            description: t(
              '使用工具自身的清理能力，保留仍被引用的内容。',
              'Use each tool’s cleanup command and retain referenced content.',
            ),
            cta: t('查看缓存', 'View caches'),
            action: () => s.go('caches'),
          },
        ]
      : []),
    ...(!settings.roots.length
      ? [
          {
            icon: FolderPlus,
            title: t('添加你的项目目录', 'Add your project folders'),
            description: t(
              '选择存放项目的位置，发现依赖与构建产物。',
              'Discover dependencies and build artifacts in your project folders.',
            ),
            cta: t('添加目录', 'Add folder'),
            action: () => void s.addRoot(),
          },
        ]
      : []),
    ...(!inventory.scannedAt
      ? [
          {
            icon: RefreshCw,
            title: t('了解这台电脑的开发环境', 'Discover your developer environments'),
            description: t(
              '扫描已安装工具、全局包和缓存。',
              'Scan installed tools, global packages, and caches.',
            ),
            cta: t('开始扫描', 'Scan now'),
            action: () => void s.refresh(),
          },
        ]
      : []),
    ...(inventory.issues.length
      ? [
          {
            icon: AlertTriangle,
            title: `${inventory.issues.length} ${t('项扫描提示', 'scan notices')}`,
            description: displayDiagnostic(inventory.issues[0]),
            cta: t('查看详情', 'View details'),
            action: () => s.go('activity'),
          },
        ]
      : []),
  ]
  const spaces = [
    {
      label: t('项目产物', 'Project artifacts'),
      icon: FolderGit2,
      color: '#3b82f6',
      bytes: inventory.projects
        .filter((p) => !p.isWorktree)
        .reduce((sum, p) => sum + projectBytes(p), 0),
      action: () => s.go('projects'),
    },
    {
      label: t('工作树产物', 'Worktree artifacts'),
      icon: GitFork,
      color: '#8b5cf6',
      bytes: inventory.projects
        .filter((p) => p.isWorktree)
        .reduce((sum, p) => sum + projectBytes(p), 0),
      action: () => s.go('projects'),
    },
    {
      label: t('全局缓存', 'Global caches'),
      icon: Database,
      color: '#f59e0b',
      bytes: cacheSize,
      action: () => s.go('caches'),
    },
    {
      label: t('模型与浏览器', 'Models & browsers'),
      icon: HardDriveDownload,
      color: '#10b981',
      bytes: inventory.providers.flatMap((p) => p.assets).reduce((sum, a) => sum + a.size.bytes, 0),
      action: () => s.go('env', null, { filter: 'downloads' }),
    },
  ]
  const total = spaces.reduce((sum, item) => sum + item.bytes, 0)
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('概览', 'Overview')}
        description={t(
          '了解当前环境状态，并按建议逐项处理。',
          'Understand your environment and work through suggested tasks.',
        )}
      />
      <div className="grid grid-cols-4 gap-4 max-lg:grid-cols-2">
        {stats.map((stat) => (
          <button
            key={stat.label}
            type="button"
            onClick={stat.action}
            className="group rounded-xl text-left focus-visible:outline-2 focus-visible:outline-ring"
            aria-label={stat.label}
          >
            <Card className="h-full gap-3 py-4 shadow-none transition-colors group-hover:border-foreground/30">
              <CardHeader className="px-4">
                <CardDescription>{stat.label}</CardDescription>
                <CardTitle className="text-2xl font-semibold tabular-nums">{stat.value}</CardTitle>
              </CardHeader>
              <CardContent className="px-4 text-xs text-muted-foreground">{stat.hint}</CardContent>
            </Card>
          </button>
        ))}
      </div>
      <div className="grid grid-cols-[minmax(0,1fr)_340px] gap-4 max-lg:grid-cols-1">
        <Card className="min-w-0 shadow-none">
          <CardHeader>
            <CardTitle>{t('建议任务', 'Suggested tasks')}</CardTitle>
            <CardDescription>
              {t(
                '按影响排序，点击进入对应页面处理',
                'Review an item to see its impact and available actions',
              )}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-2">
            {tasks.length ? (
              tasks.map((task) => (
                <button
                  key={task.title}
                  type="button"
                  onClick={task.action}
                  disabled={
                    (task.icon === FolderPlus || task.icon === RefreshCw) &&
                    (s.busy || !s.api.native)
                  }
                  title={
                    task.icon === FolderPlus || task.icon === RefreshCw
                      ? s.busy
                        ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
                        : !s.api.native
                          ? t('请在桌面应用中执行扫描。', 'Scan from the desktop application.')
                          : undefined
                      : undefined
                  }
                  className="flex w-full items-center gap-4 rounded-lg border p-3 text-left transition-colors hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-ring disabled:cursor-not-allowed disabled:opacity-50"
                >
                  <div className="flex size-9 shrink-0 items-center justify-center rounded-md bg-muted">
                    <task.icon className="size-4 text-muted-foreground" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="text-sm font-medium">{task.title}</div>
                    <div className="truncate text-xs text-muted-foreground">{task.description}</div>
                  </div>
                  <span className="flex shrink-0 items-center gap-2 text-xs font-medium">
                    {task.cta}
                    <ArrowRight className="size-3.5" />
                  </span>
                </button>
              ))
            ) : (
              <Empty>
                {t('一切井然有序，暂无待处理任务。', 'Everything is in order. No tasks to review.')}
              </Empty>
            )}
          </CardContent>
        </Card>
        <Card className="min-w-0 shadow-none">
          <CardHeader>
            <CardTitle>{t('空间分布', 'Space distribution')}</CardTitle>
            <CardDescription>
              {t(
                '项目产物、缓存与下载资源的逻辑大小',
                'Logical size of artifacts, caches, and downloads',
              )}
            </CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {spaces.map(({ label, bytes, color, icon: Icon, action }) => (
              <button
                key={label}
                type="button"
                onClick={action}
                className="block w-full space-y-1.5 rounded-md p-2 text-left transition-colors hover:bg-muted/50 focus-visible:outline-2 focus-visible:outline-ring"
              >
                <div className="flex items-center gap-2 text-sm">
                  <Icon className="size-3.5" style={{ color }} />
                  {label}
                  <span className="ml-auto font-mono text-xs text-muted-foreground">
                    {formatBytes(bytes)}
                  </span>
                </div>
                <div className="h-1.5 overflow-hidden rounded-full bg-muted">
                  <div
                    className="h-full rounded-full transition-all"
                    style={{
                      width: `${total ? (bytes / total) * 100 : 0}%`,
                      background: color,
                    }}
                  />
                </div>
              </button>
            ))}
          </CardContent>
        </Card>
      </div>
      <div className="grid grid-cols-2 gap-4 max-lg:grid-cols-1">
        <Card className="min-w-0 shadow-none">
          <CardHeader>
            <CardTitle>{t('当前运行时', 'Current runtimes')}</CardTitle>
            <CardAction>
              <Button variant="ghost" size="sm" onClick={() => s.go('env', null)}>
                {t('全部环境', 'All environments')}
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="divide-y">
            {inventory.providers
              .filter((p) => p.runtimes.length)
              .map((p) => (
                <button
                  key={p.id}
                  onClick={() => s.go('env', p.id)}
                  className="flex w-full items-center gap-3 py-2.5 text-left text-sm first:pt-0 last:pb-0"
                >
                  <EcoDot id={p.id} />
                  <span className="w-20">{metadata[p.id].runtime}</span>
                  <span className="truncate font-mono text-xs text-muted-foreground">
                    {p.runtimes.find((r) => r.active)?.version ??
                      t('未设置默认', 'No default detected')}
                  </span>
                </button>
              ))}
            {!inventory.providers.some((p) => p.runtimes.length) && (
              <Empty>
                {t('扫描后显示已安装的运行时。', 'Installed runtimes appear after a scan.')}
              </Empty>
            )}
          </CardContent>
        </Card>
        <Card className="min-w-0 shadow-none">
          <CardHeader>
            <CardTitle>{t('最近操作', 'Recent activity')}</CardTitle>
            <CardAction>
              <Button variant="ghost" size="sm" onClick={() => s.go('activity')}>
                {t('全部', 'View all')}
              </Button>
            </CardAction>
          </CardHeader>
          <CardContent className="divide-y">
            {activity.slice(0, 5).map((item) => (
              <div key={item.id} className="flex items-center gap-3 py-2.5 text-sm">
                <span className="flex-1 truncate">{item.title}</span>
                {item.removedBytes > 0 && (
                  <span className="font-mono text-xs text-emerald-600">
                    {t('移除（逻辑）', 'Removed (logical)')} {formatBytes(item.removedBytes)}
                  </span>
                )}
                <span className="text-xs text-muted-foreground">
                  {new Date(item.time * 1000).toLocaleDateString()}
                </span>
              </div>
            ))}
            {!activity.length && (
              <Empty>
                {t('操作记录会保存在这台电脑上。', 'Activity is stored locally on this computer.')}
              </Empty>
            )}
          </CardContent>
        </Card>
      </div>
    </div>
  )
}
