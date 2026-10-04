import { FolderGit2, GitBranch, GitFork, LockKeyhole, ShieldCheck } from 'lucide-react'
import { formatBytes, metadata, projectBytes, type Artifact, type Project } from '@/domain'
import { useStore } from '@/store'
import { EcoDot } from './shared'
import { ActionButton, SelectionCheckbox } from './action-controls'
import { Badge } from './ui/badge'
import { Card, CardContent } from './ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table'

export function cleanupReason(
  project: Project,
  artifact: Artifact,
  t: (zh: string, en: string) => string,
) {
  if (project.protected)
    return t(
      '项目已保护，取消保护后才能清理。',
      'Project is protected. Unprotect it before cleanup.',
    )
  if (!artifact.canClean)
    return (
      artifact.cleanupIssue ||
      t(
        '无法确认此目录由工具生成，请使用原工具清理。',
        'Generated-directory ownership is unverified. Use the owning tool to clean it.',
      )
    )
  if (!artifact.size.complete)
    return t(
      '目录扫描不完整，请检查权限后重新扫描。',
      'Directory scan is incomplete. Check permissions and rescan.',
    )
  return null
}

function relativePath(project: Project, artifact: Artifact) {
  const root = project.path.replaceAll('\\', '/').replace(/\/$/, '')
  const path = artifact.path.replaceAll('\\', '/')
  return path.startsWith(`${root}/`) ? path.slice(root.length + 1) : artifact.name
}

export function ProjectTable({
  projects,
  selected,
  toggle,
  label,
}: {
  projects: Project[]
  selected: Set<string>
  toggle(ids: string[], on: boolean): void
  label?: string
}) {
  const s = useStore()
  const { t } = s
  const busyReason = s.busy
    ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
    : null
  const ids = projects.flatMap((p) =>
    p.artifacts.filter((a) => !cleanupReason(p, a, t)).map((a) => a.id),
  )
  const count = ids.filter((id) => selected.has(id)).length
  const protect = async (project: Project) => {
    const settings = s.data.settings
    const saved = await s.saveSettings({
      ...settings,
      protectedProjects: project.protected
        ? settings.protectedProjects.filter((p) => p !== project.path)
        : [...settings.protectedProjects, project.path],
    })
    if (saved) await s.refresh()
  }
  return (
    <Card className="overflow-hidden py-0 shadow-none">
      <CardContent className="px-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-10 pl-4">
                <SelectionCheckbox
                  aria-label={`${t('选择所有可清理项目', 'Select all eligible artifacts')}${label ? ` · ${label}` : ''}`}
                  reason={
                    busyReason ||
                    (!ids.length
                      ? t(
                          '当前列表没有可清理目录，原因见各行说明。',
                          'No eligible directories in this list. See the reasons in each row.',
                        )
                      : null)
                  }
                  checked={count && count === ids.length ? true : count ? 'indeterminate' : false}
                  onCheckedChange={(on) => toggle(ids, on === true)}
                />
              </TableHead>
              <TableHead>{t('仓库 / 工作树', 'Repository / worktree')}</TableHead>
              <TableHead>{t('最近活动', 'Last activity')}</TableHead>
              <TableHead>{t('可清理目录', 'Artifacts')}</TableHead>
              <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {projects.map((project) => {
              const available = project.artifacts.filter((a) => !cleanupReason(project, a, t))
              const count = available.filter((a) => selected.has(a.id)).length
              const unavailable = project.protected
                ? t('项目已保护', 'Project is protected')
                : t('没有可安全清理的目录', 'No eligible directories')
              return (
                <TableRow key={project.id}>
                  <TableCell className="pl-4">
                    <SelectionCheckbox
                      aria-label={`${t('选择', 'Select')} ${project.name}`}
                      reason={busyReason || (!available.length ? unavailable : null)}
                      checked={
                        count && count === available.length ? true : count ? 'indeterminate' : false
                      }
                      onCheckedChange={(on) =>
                        toggle(
                          available.map((a) => a.id),
                          on === true,
                        )
                      }
                    />
                  </TableCell>
                  <TableCell>
                    <div className="flex items-center gap-2 font-medium">
                      {project.isWorktree ? (
                        <GitFork className="size-4 text-muted-foreground" />
                      ) : project.providers[0] ? (
                        <EcoDot id={project.providers[0]} />
                      ) : (
                        <FolderGit2 className="size-4 text-muted-foreground" />
                      )}
                      {project.name}
                      {project.protected && <ShieldCheck className="size-3.5 text-emerald-600" />}
                    </div>
                    <p
                      className="mt-1 max-w-72 truncate font-mono text-xs text-muted-foreground"
                      title={project.path}
                    >
                      {project.path}
                    </p>
                    <div className="mt-1 flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
                      <GitBranch className="size-3" />
                      {project.branch ?? t('分支未知', 'Unknown branch')}
                      {s.data.inventory.worktrees?.some((w) => w.id === project.id && w.locked) && (
                        <Badge variant="outline">
                          <LockKeyhole className="size-3" />
                          {t('锁定', 'Locked')}
                        </Badge>
                      )}
                      {project.providers.map((id) => (
                        <Badge key={id} variant="outline" className="text-[10px] font-normal">
                          {metadata[id].short}
                        </Badge>
                      ))}
                    </div>
                    {!available.length && (
                      <p className="mt-2 text-xs text-muted-foreground">{unavailable}</p>
                    )}
                  </TableCell>
                  <TableCell className="text-xs text-muted-foreground">
                    {project.lastActive
                      ? new Date(project.lastActive * 1000).toLocaleDateString()
                      : t('未知', 'Unknown')}
                    {!project.activityComplete && <p>{t('扫描不完整', 'Partial scan')}</p>}
                  </TableCell>
                  <TableCell>
                    <div className="min-w-40 max-w-72 space-y-2">
                      {project.artifacts.map((artifact) => {
                        const reason = cleanupReason(project, artifact, t)
                        return (
                          <div key={artifact.id}>
                            <label
                              className={`flex items-center gap-2 text-xs ${reason ? 'text-muted-foreground' : 'cursor-pointer'}`}
                              title={artifact.path}
                            >
                              <SelectionCheckbox
                                className="size-3.5"
                                aria-label={relativePath(project, artifact)}
                                reason={busyReason || reason}
                                checked={!reason && selected.has(artifact.id)}
                                onCheckedChange={(on) => toggle([artifact.id], on === true)}
                              />
                              <span className="break-all font-mono">
                                {relativePath(project, artifact)}
                              </span>
                            </label>
                            {reason && (
                              <p className="mt-1 text-xs text-muted-foreground">{reason}</p>
                            )}
                          </div>
                        )
                      })}
                      {!project.artifacts.length && (
                        <span className="text-xs text-muted-foreground">
                          {t('未发现可重建产物', 'No generated artifacts found')}
                        </span>
                      )}
                    </div>
                  </TableCell>
                  <TableCell className="text-right font-mono text-xs">
                    {project.artifacts.some((a) => !a.size.complete) && '≥ '}
                    {formatBytes(projectBytes(project))}
                  </TableCell>
                  <TableCell className="text-right">
                    <ActionButton
                      variant="ghost"
                      size="icon"
                      reason={busyReason}
                      aria-label={
                        project.protected
                          ? t('取消保护', 'Unprotect project')
                          : t('保护项目', 'Protect project')
                      }
                      onClick={() => void protect(project)}
                    >
                      <ShieldCheck className={project.protected ? 'text-emerald-600' : ''} />
                    </ActionButton>
                  </TableCell>
                </TableRow>
              )
            })}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  )
}
