import { useState } from 'react'
import { FolderPlus, ExternalLink, ShieldCheck, Trash2 } from 'lucide-react'
import { useStore } from '@/store'
import type { Settings as Preferences } from '@/domain'
import { PageHeader } from '@/components/shared'
import { Button } from '@/components/ui/button'
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card'
import { Input } from '@/components/ui/input'
import { Switch } from '@/components/ui/switch'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'
import { Textarea } from '@/components/ui/textarea'

export default function Settings() {
  const s = useStore()
  const { t } = s
  const settings = s.data.settings
  const [excludes, setExcludes] = useState(settings.excludes.join('\n'))
  const [days, setDays] = useState(String(settings.idleDays))
  const patch = (value: Partial<Preferences>) => void s.saveSettings({ ...settings, ...value })
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('设置', 'Settings')}
        description={t(
          '让 Envark 按你的习惯管理开发环境。',
          'Make Envark work with your development habits.',
        )}
      />
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>{t('通用', 'General')}</CardTitle>
          <CardDescription>
            {t('外观、语言与启动行为', 'Appearance, language, and startup behavior')}
          </CardDescription>
        </CardHeader>
        <CardContent className="divide-y">
          <Row title={t('语言', 'Language')}>
            <Select
              value={settings.language}
              onValueChange={(language) => patch({ language: language as Preferences['language'] })}
              disabled={s.busy}
            >
              <SelectTrigger className="w-44" aria-label={t('语言', 'Language')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="zh-CN">简体中文</SelectItem>
                <SelectItem value="zh-TW">繁體中文</SelectItem>
                <SelectItem value="en">English</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          <Row title={t('外观', 'Appearance')}>
            <Select
              value={settings.theme}
              onValueChange={(theme) => patch({ theme: theme as Preferences['theme'] })}
              disabled={s.busy}
            >
              <SelectTrigger className="w-44" aria-label={t('外观', 'Appearance')}>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="light">{t('浅色', 'Light')}</SelectItem>
                <SelectItem value="dark">{t('深色', 'Dark')}</SelectItem>
                <SelectItem value="system">{t('跟随系统', 'System')}</SelectItem>
              </SelectContent>
            </Select>
          </Row>
          <Row
            title={t('启动时扫描', 'Scan on launch')}
            description={t(
              '打开应用后刷新工具与项目状态。',
              'Refresh environments and projects when the application opens.',
            )}
          >
            <Switch
              aria-label={t('启动时扫描', 'Scan on launch')}
              checked={settings.scanOnLaunch}
              disabled={s.busy}
              onCheckedChange={(scanOnLaunch) => patch({ scanOnLaunch })}
            />
          </Row>
          <Row
            title={t('检查工具更新', 'Check tool updates')}
            description={t(
              '允许查询公共包仓库中的最新版本。',
              'Allow version checks against public package registries.',
            )}
          >
            <Switch
              aria-label={t('检查工具更新', 'Check tool updates')}
              checked={settings.checkUpdates}
              disabled={s.busy}
              onCheckedChange={(checkUpdates) => patch({ checkUpdates })}
            />
          </Row>
        </CardContent>
      </Card>
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>{t('项目扫描', 'Project scanning')}</CardTitle>
          <CardDescription>
            {t(
              '仅扫描你选择的目录。重叠目录自动去重。',
              'Only scan folders you choose. Overlapping roots are deduplicated.',
            )}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="divide-y rounded-lg border">
            {settings.roots.map((root) => (
              <div key={root} className="flex items-center gap-3 p-3">
                <span className="min-w-0 flex-1 truncate font-mono text-xs" title={root}>
                  {root}
                </span>
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label={`${t('移除扫描目录', 'Remove scan folder')} ${root}`}
                  disabled={s.busy}
                  onClick={() => patch({ roots: settings.roots.filter((p) => p !== root) })}
                >
                  <Trash2 />
                </Button>
              </div>
            ))}
            {!settings.roots.length && (
              <p className="p-4 text-sm text-muted-foreground">
                {t('尚未添加扫描目录。', 'No scan folders yet.')}
              </p>
            )}
          </div>
          <Button variant="outline" disabled={s.busy} onClick={() => void s.addRoot()}>
            <FolderPlus />
            {t('添加目录', 'Add folder')}
          </Button>
          <label className="block space-y-2 text-sm">
            <span>
              {t('忽略规则（每行一条，可使用通配符）', 'Ignore rules (one glob per line)')}
            </span>
            <Textarea
              value={excludes}
              onChange={(e) => setExcludes(e.target.value)}
              className="font-mono text-xs"
            />
          </label>
          <Button
            variant="outline"
            disabled={s.busy}
            onClick={() =>
              patch({
                excludes: excludes
                  .split('\n')
                  .map((v) => v.trim())
                  .filter(Boolean),
              })
            }
          >
            {t('保存忽略规则', 'Save ignore rules')}
          </Button>
        </CardContent>
      </Card>
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>{t('清理与安全', 'Cleanup & safety')}</CardTitle>
        </CardHeader>
        <CardContent className="divide-y">
          <Row
            title={t('闲置阈值', 'Inactivity threshold')}
            description={t(
              '超过这个天数的项目出现在闲置建议中。',
              'Projects older than this threshold appear in idle suggestions.',
            )}
          >
            <div className="flex items-center gap-2">
              <Input
                type="number"
                min={1}
                max={3650}
                value={days}
                onChange={(e) => setDays(e.target.value)}
                className="w-20"
                aria-label={t('闲置天数', 'Idle days')}
              />
              <span className="text-sm text-muted-foreground">{t('天', 'days')}</span>
              <Button
                variant="outline"
                size="sm"
                disabled={
                  s.busy ||
                  !Number.isInteger(Number(days)) ||
                  Number(days) < 1 ||
                  Number(days) > 3650
                }
                onClick={() => patch({ idleDays: Number(days) })}
              >
                {t('保存', 'Save')}
              </Button>
            </div>
          </Row>
          <Row
            title={t('移至回收站', 'Move to Trash')}
            description={t(
              '项目产物与浏览器资源默认可从系统回收站恢复。',
              'Project artifacts and browser resources can be restored from the system Trash.',
            )}
          >
            <Switch
              aria-label={t('移至回收站', 'Move to Trash')}
              checked={settings.useTrash}
              disabled={s.busy}
              onCheckedChange={(useTrash) => patch({ useTrash })}
            />
          </Row>
          <div className="flex items-start gap-3 pt-4 text-xs text-muted-foreground">
            <ShieldCheck className="size-4 shrink-0" />
            <p>
              {t(
                '源码、锁文件、版本声明、环境变量文件与 Git 数据不会作为清理目标。受保护的项目不可清理。',
                'Source files, lockfiles, version pins, environment files, and Git data are never cleanup targets. Protected projects cannot be cleaned.',
              )}
            </p>
          </div>
        </CardContent>
      </Card>
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>{t('版本管理器', 'Version managers')}</CardTitle>
          <CardDescription>
            {t('与环境页面的优先设置同步。', 'Kept in sync with the environment pages.')}
          </CardDescription>
        </CardHeader>
        <CardContent className="divide-y">
          {s.data.inventory.providers
            .filter((p) => p.managers.length)
            .map((p) => (
              <Row key={p.id} title={p.managers.map((m) => m.name).join(' / ')}>
                <Select
                  value={settings.preferred[p.id] ?? p.managers[0]?.name}
                  onValueChange={(manager) =>
                    patch({ preferred: { ...settings.preferred, [p.id]: manager } })
                  }
                  disabled={s.busy}
                >
                  <SelectTrigger
                    className="w-44"
                    aria-label={`${t('优先管理器', 'Preferred manager')} ${p.id}`}
                  >
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {p.managers.map((m) => (
                      <SelectItem key={m.name} value={m.name}>
                        {m.name}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Row>
            ))}
          {!s.data.inventory.providers.some((p) => p.managers.length) && (
            <p className="text-sm text-muted-foreground">
              {t('扫描后显示可用管理器。', 'Available managers appear after a scan.')}
            </p>
          )}
        </CardContent>
      </Card>
      <Card className="shadow-none">
        <CardHeader>
          <CardTitle>{t('关于', 'About')} Envark</CardTitle>
          <CardDescription>
            {t('开发工具与空间的归处。', 'A considered home for developer tools and space.')}
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-3 text-sm">
          <p>
            Envark {s.data.version} · {s.data.platform || 'desktop'}
          </p>
          <div>
            <p className="text-muted-foreground">{t('数据与操作记录', 'Data & activity')}</p>
            <p className="mt-1 break-all font-mono text-xs">
              {s.data.dataDir || t('桌面应用中可查看', 'Available in the desktop app')}
            </p>
          </div>
          <a
            className="inline-flex items-center gap-2 text-sm underline underline-offset-4"
            href="https://github.com/felixpy/envark"
            target="_blank"
            rel="noreferrer"
          >
            <ExternalLink className="size-4" />
            GitHub
          </a>
        </CardContent>
      </Card>
    </div>
  )
}

function Row({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: React.ReactNode
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-4 first:pt-0 last:pb-0">
      <div className="space-y-1">
        <div className="text-sm font-medium">{title}</div>
        {description && <p className="text-xs text-muted-foreground">{description}</p>}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  )
}
