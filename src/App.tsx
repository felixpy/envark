import { lazy, Suspense } from 'react'
import type { Backend } from './bridge'
import {
  Boxes,
  Database,
  FolderGit2,
  HardDrive,
  History,
  LayoutDashboard,
  RefreshCw,
  Settings as SettingsIcon,
  Terminal,
  X,
} from 'lucide-react'
import { Toaster } from 'sonner'
import { StoreProvider, useStore } from './store'
import { categories, metadata, providerIds, formatBytes, type View } from './domain'
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuBadge,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from './components/ui/sidebar'
import { TooltipProvider } from './components/ui/tooltip'
import { Separator } from './components/ui/separator'
import { Progress } from './components/ui/progress'
import { Button } from './components/ui/button'
import { EcoDot, Spinner } from './components/shared'
import { OperationDialog } from './components/OperationDialog'
const Overview = lazy(() => import('./views/Overview'))
const Catalog = lazy(() => import('./views/Catalog'))
const Environments = lazy(() => import('./views/Environments'))
const Projects = lazy(() => import('./views/Projects'))
const Caches = lazy(() => import('./views/Caches'))
const Activity = lazy(() => import('./views/Activity'))
const Settings = lazy(() => import('./views/Settings'))

function Shell() {
  const s = useStore()
  const { t, data } = s
  const navigation = [
    { id: 'overview', label: t('概览', 'Overview'), icon: LayoutDashboard },
    { id: 'env', label: t('环境与工具', 'Environments & tools'), icon: Boxes },
    { id: 'projects', label: t('项目空间', 'Project space'), icon: FolderGit2 },
    { id: 'caches', label: t('全局缓存', 'Global caches'), icon: Database },
    { id: 'activity', label: t('操作记录', 'Activity'), icon: History },
  ] as const
  const current = navigation.find((n) => n.id === s.view)?.label ?? t('设置', 'Settings')
  const disk =
    data.inventory.disks.find((d) => data.dataDir.startsWith(d.mount)) ?? data.inventory.disks[0]
  return (
    <TooltipProvider>
      <SidebarProvider>
        <Sidebar variant="inset">
          <SidebarHeader>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton size="lg" onClick={() => s.go('overview')}>
                  <div className="flex size-8 items-center justify-center rounded-lg bg-primary text-primary-foreground">
                    <Terminal className="size-4" />
                  </div>
                  <div className="grid flex-1 text-left leading-tight">
                    <span className="text-sm font-semibold">Envark</span>
                    <span className="text-xs text-muted-foreground">
                      {t('开发环境管家', 'A home for your dev tools')}
                    </span>
                  </div>
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
          </SidebarHeader>
          <SidebarContent>
            <SidebarGroup>
              <SidebarGroupContent>
                <SidebarMenu>
                  {navigation.map((item) => (
                    <SidebarMenuItem key={item.id}>
                      <SidebarMenuButton
                        isActive={s.view === item.id && !(item.id === 'env' && s.provider)}
                        onClick={() => s.go(item.id, item.id === 'env' ? null : undefined)}
                      >
                        <item.icon />
                        {item.label}
                      </SidebarMenuButton>
                    </SidebarMenuItem>
                  ))}
                </SidebarMenu>
              </SidebarGroupContent>
            </SidebarGroup>
            {categories.map((category) => (
              <SidebarGroup key={category.id} className="py-1">
                <SidebarGroupLabel>{t(category.zh, category.en)}</SidebarGroupLabel>
                <SidebarGroupContent>
                  <SidebarMenu>
                    {providerIds
                      .filter((id) => metadata[id].category === category.id)
                      .map((id) => {
                        const provider = data.inventory.providers.find((p) => p.id === id)
                        const active = provider?.runtimes.find((r) => r.active)
                        return (
                          <SidebarMenuItem key={id}>
                            <SidebarMenuButton
                              size="sm"
                              isActive={s.view === 'env' && s.provider === id}
                              onClick={() => s.go('env', id)}
                            >
                              <EcoDot id={id} />
                              {metadata[id].short}
                            </SidebarMenuButton>
                            <SidebarMenuBadge className="max-w-24 truncate font-mono text-[11px] font-normal text-muted-foreground">
                              {active?.version.split(' ')[0] ?? (provider?.assets.length || '')}
                            </SidebarMenuBadge>
                          </SidebarMenuItem>
                        )
                      })}
                  </SidebarMenu>
                </SidebarGroupContent>
              </SidebarGroup>
            ))}
          </SidebarContent>
          <SidebarFooter>
            <SidebarMenu>
              <SidebarMenuItem>
                <SidebarMenuButton
                  isActive={s.view === 'settings'}
                  onClick={() => s.go('settings')}
                >
                  <SettingsIcon />
                  {t('设置', 'Settings')}
                </SidebarMenuButton>
              </SidebarMenuItem>
            </SidebarMenu>
            <div className="space-y-2 rounded-lg border bg-background p-3">
              <div className="flex items-center gap-2 truncate text-xs font-medium">
                <HardDrive className="size-3.5 text-muted-foreground" />
                {disk?.name || t('本机存储', 'Local storage')}
              </div>
              <Progress
                value={disk ? ((disk.total - disk.available) / disk.total) * 100 : 0}
                className="h-1.5"
              />
              <div className="flex justify-between text-xs text-muted-foreground">
                {disk ? (
                  <>
                    <span>
                      {formatBytes(disk.total - disk.available)} / {formatBytes(disk.total)}
                    </span>
                    <span>{t('已使用', 'Used')}</span>
                  </>
                ) : (
                  <span>{t('扫描后显示磁盘信息', 'Scan to inspect disk usage')}</span>
                )}
              </div>
            </div>
          </SidebarFooter>
        </Sidebar>
        <SidebarInset>
          <header className="flex h-14 shrink-0 items-center gap-2 border-b px-4">
            <SidebarTrigger className="-ml-1" />
            <Separator
              orientation="vertical"
              className="mr-2 self-center data-[orientation=vertical]:h-4"
            />
            <span className="text-sm text-muted-foreground">Envark</span>
            <span className="text-sm text-muted-foreground">/</span>
            {s.view === 'env' && s.provider ? (
              <>
                <button className="text-sm text-muted-foreground" onClick={() => s.go('env', null)}>
                  {current}
                </button>
                <span className="text-sm text-muted-foreground">/</span>
                <span className="text-sm font-medium">{metadata[s.provider].name}</span>
              </>
            ) : (
              <span className="text-sm font-medium">{current}</span>
            )}
            <div className="ml-auto flex items-center gap-2">
              {s.busy ? (
                <>
                  <Spinner />
                  <span className="max-w-64 truncate text-xs text-muted-foreground">
                    {s.progress?.message ?? t('正在扫描', 'Scanning')}
                  </span>
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label={t('取消', 'Cancel')}
                    onClick={() => void s.cancel()}
                  >
                    <X />
                  </Button>
                </>
              ) : (
                <Button
                  variant="ghost"
                  size="sm"
                  disabled={!s.api.native}
                  onClick={() => void s.refresh()}
                >
                  <RefreshCw />
                  {t('刷新', 'Refresh')}
                </Button>
              )}
            </div>
          </header>
          {!s.api.native && (
            <div className="border-b bg-muted/50 px-6 py-2 text-xs text-muted-foreground">
              {t(
                '界面预览 · 请打开桌面应用以读取和管理本机环境。',
                'Interface preview · Open the desktop app to inspect and manage this computer.',
              )}
            </div>
          )}
          {s.error && (
            <div
              role="alert"
              className="border-b border-destructive/20 bg-destructive/5 px-6 py-3 text-sm text-destructive"
            >
              {s.error}
            </div>
          )}
          <main className="mx-auto w-full max-w-6xl flex-1 p-6 lg:p-8">
            {!s.loaded ? (
              <div className="flex gap-2 text-sm">
                <Spinner />
                {t('加载中…', 'Loading…')}
              </div>
            ) : (
              <Suspense fallback={<Spinner />}>
                <Page view={s.view} />
              </Suspense>
            )}
          </main>
        </SidebarInset>
      </SidebarProvider>
      <OperationDialog />
      <Toaster position="bottom-right" />
    </TooltipProvider>
  )
}

function Page({ view }: { view: View }) {
  const s = useStore()
  switch (view) {
    case 'overview':
      return <Overview />
    case 'env':
      return s.provider ? <Environments key={s.provider} id={s.provider} /> : <Catalog />
    case 'projects':
      return <Projects />
    case 'caches':
      return <Caches />
    case 'activity':
      return <Activity />
    case 'settings':
      return <Settings />
  }
}

export default function App({ api }: { api?: Backend }) {
  return (
    <StoreProvider api={api}>
      <Shell />
    </StoreProvider>
  )
}
