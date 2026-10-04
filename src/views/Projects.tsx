import { useState } from 'react'
import { FolderPlus, GitBranch, RefreshCw, ShieldCheck, Trash2 } from 'lucide-react'
import { formatBytes, idle, metadata, projectBytes, type ProviderId } from '@/domain'
import { useStore } from '@/store'
import { EcoDot, Empty, PageHeader, SearchInput } from '@/components/shared'
import { Button } from '@/components/ui/button'
import { Card, CardContent } from '@/components/ui/card'
import { Badge } from '@/components/ui/badge'
import { Checkbox } from '@/components/ui/checkbox'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/components/ui/table'

export default function Projects() {
  const s = useStore()
  const { t } = s
  const { inventory, settings } = s.data
  const [query, setQuery] = useState('')
  const [provider, setProvider] = useState('all')
  const [idleOnly, setIdleOnly] = useState(false)
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const list = inventory.projects.filter(
    (p) =>
      (provider === 'all' || p.providers.includes(provider as ProviderId)) &&
      (!idleOnly || idle(p, settings.idleDays)) &&
      `${p.name} ${p.path}`.toLowerCase().includes(query.trim().toLowerCase()),
  )
  const eligible = list.flatMap((p) =>
    p.protected ? [] : p.artifacts.filter((a) => a.canClean && a.size.complete),
  )
  const checked = eligible.length > 0 && eligible.every((a) => selected.has(a.id))
  const toggle = (ids: string[], on: boolean) =>
    setSelected((current) => {
      const next = new Set(current)
      for (const id of ids) {
        if (on) next.add(id)
        else next.delete(id)
      }
      return next
    })
  const chosen = eligible.filter((a) => selected.has(a.id))
  const protect = (path: string, on: boolean) =>
    void s
      .saveSettings({
        ...settings,
        protectedProjects: on
          ? [...settings.protectedProjects, path]
          : settings.protectedProjects.filter((p) => p !== path),
      })
      .then((saved) => {
        if (saved) void s.refresh()
      })
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('项目空间', 'Project space')}
        description={t(
          '清理可重建的依赖、虚拟环境与构建产物，保留源码和关键配置。',
          'Review regenerable dependencies, environments, and build artifacts while preserving source and configuration.',
        )}
        actions={
          <>
            <Button variant="outline" disabled={s.busy} onClick={() => void s.addRoot()}>
              <FolderPlus />
              {t('添加目录', 'Add folder')}
            </Button>
            <Button variant="outline" disabled={s.busy} onClick={() => void s.refresh()}>
              <RefreshCw />
              {t('重新扫描', 'Rescan')}
            </Button>
          </>
        }
      />
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={t('搜索项目', 'Search projects')}
        />
        <Tabs value={provider} onValueChange={setProvider}>
          <TabsList>
            <TabsTrigger value="all">{t('全部', 'All')}</TabsTrigger>
            {(['js', 'py', 'jvm', 'rust', 'go'] as const).map((id) => (
              <TabsTrigger key={id} value={id}>
                {metadata[id].short}
              </TabsTrigger>
            ))}
          </TabsList>
        </Tabs>
        <label className="ml-auto flex items-center gap-2 text-sm text-muted-foreground">
          <Switch checked={idleOnly} onCheckedChange={setIdleOnly} />
          {t('仅闲置', 'Idle only')} &gt; {settings.idleDays} {t('天', 'days')}
        </label>
      </div>
      <Card className="overflow-hidden py-0 shadow-none">
        <CardContent className="px-0">
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead className="w-10 pl-4">
                  <Checkbox
                    aria-label={t('选择所有可清理项目', 'Select all eligible artifacts')}
                    checked={checked ? true : chosen.length ? 'indeterminate' : false}
                    onCheckedChange={(on) =>
                      toggle(
                        eligible.map((a) => a.id),
                        on === true,
                      )
                    }
                  />
                </TableHead>
                <TableHead>{t('项目', 'Project')}</TableHead>
                <TableHead>{t('最近活动', 'Last activity')}</TableHead>
                <TableHead>{t('可清理目录', 'Artifacts')}</TableHead>
                <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {list.map((project) => (
                <TableRow key={project.id}>
                  <TableCell className="pl-4">
                    <Checkbox
                      aria-label={`${t('选择', 'Select')} ${project.name}`}
                      disabled={
                        project.protected ||
                        !project.artifacts.some((a) => a.canClean && a.size.complete)
                      }
                      checked={
                        project.artifacts.some((a) => a.canClean && a.size.complete) &&
                        project.artifacts
                          .filter((a) => a.canClean && a.size.complete)
                          .every((a) => selected.has(a.id))
                      }
                      onCheckedChange={(on) =>
                        toggle(
                          project.artifacts
                            .filter((a) => a.canClean && a.size.complete)
                            .map((a) => a.id),
                          on === true,
                        )
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2 font-medium">
                      <EcoDot id={project.providers[0]} />
                      {project.name}
                      {project.protected && <ShieldCheck className="size-3.5 text-emerald-600" />}
                    </div>
                    <div
                      className="mt-1 max-w-80 truncate font-mono text-xs text-muted-foreground"
                      title={project.path}
                    >
                      {project.path}
                    </div>
                    {project.branch && (
                      <div className="mt-1 flex items-center gap-1 text-xs text-muted-foreground">
                        <GitBranch className="size-3" />
                        {project.branch}
                      </div>
                    )}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {project.lastActive
                      ? new Date(project.lastActive * 1000).toLocaleDateString()
                      : t('未知', 'Unknown')}
                    {!project.activityComplete && <p>{t('扫描不完整', 'Partial scan')}</p>}
                  </TableCell>
                  <TableCell>
                    <div className="flex max-w-64 flex-wrap gap-1">
                      {project.artifacts.map((artifact) => (
                        <label
                          key={artifact.id}
                          title={artifact.cleanupIssue ?? undefined}
                          className="flex cursor-pointer items-center gap-1 rounded border px-1.5 py-1 text-xs"
                        >
                          <Checkbox
                            className="size-3"
                            checked={selected.has(artifact.id)}
                            disabled={
                              project.protected || !artifact.canClean || !artifact.size.complete
                            }
                            onCheckedChange={(on) => toggle([artifact.id], on === true)}
                          />
                          {artifact.name}
                          {!artifact.canClean && (
                            <span className="text-muted-foreground">{t('只读', 'Read-only')}</span>
                          )}
                        </label>
                      ))}
                    </div>
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {formatBytes(projectBytes(project))}
                  </TableCell>
                  <TableCell className="text-right">
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label={
                        project.protected
                          ? t('取消保护', 'Unprotect project')
                          : t('保护项目', 'Protect project')
                      }
                      disabled={s.busy}
                      onClick={() => protect(project.path, !project.protected)}
                    >
                      <ShieldCheck className={project.protected ? 'text-emerald-600' : ''} />
                    </Button>
                  </TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {!list.length && (
            <Empty>
              {settings.roots.length
                ? t(
                    '没有符合条件的项目。点击重新扫描获取最新结果。',
                    'No matching projects. Rescan to refresh the inventory.',
                  )
                : t('添加项目目录后开始扫描。', 'Add a project folder to get started.')}
            </Empty>
          )}
        </CardContent>
      </Card>
      {chosen.length > 0 && (
        <div className="sticky bottom-4 flex items-center justify-between rounded-xl border bg-background p-4 shadow-lg">
          <span className="text-sm">
            {chosen.length} {t('个目录', 'directories')} ·{' '}
            <span className="font-mono">
              {formatBytes(chosen.reduce((sum, a) => sum + a.size.bytes, 0))}
            </span>
          </span>
          <Button
            disabled={s.busy}
            onClick={() =>
              void s.prepare({ kind: 'cleanProjects', artifactIds: chosen.map((a) => a.id) })
            }
          >
            <Trash2 />
            {t('审阅清理', 'Review cleanup')}
          </Button>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span>{t('扫描范围', 'Scan roots')}:</span>
        {settings.roots.map((root) => (
          <Badge key={root} variant="outline" className="font-mono font-normal">
            {root}
          </Badge>
        ))}
        <span>
          {t(
            '活动依据：源文件修改时间与 Git 活动；不代表项目可用性。',
            'Activity uses source timestamps and Git activity; it is not a project health assessment.',
          )}
        </span>
      </div>
    </div>
  )
}
