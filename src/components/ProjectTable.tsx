import { Fragment } from 'react'
import {
  ChevronRight,
  FolderGit2,
  GitBranch,
  GitFork,
  LockKeyhole,
  ShieldCheck,
  Trash2,
} from 'lucide-react'
import { formatBytes, metadata, projectBytes, type Artifact, type Project } from '@/domain'
import {
  cleanupReason,
  eligibleArtifacts,
  groupWorkspaces,
  type ProjectGroup,
  type WorkspaceNode,
} from '@/lib/project-tree'
import { useStore } from '@/store'
import { EcoDot } from './shared'
import { ActionButton, SelectionCheckbox } from './action-controls'
import { Badge } from './ui/badge'
import { Button } from './ui/button'
import { Card, CardContent } from './ui/card'
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from './ui/table'

function relativePath(project: Project, artifact: Artifact) {
  const root = project.path.replaceAll('\\', '/').replace(/\/$/, '')
  const path = artifact.path.replaceAll('\\', '/')
  return path.startsWith(`${root}/`) ? path.slice(root.length + 1) : artifact.name
}

function selectionState(ids: string[], selected: Set<string>) {
  const count = ids.filter((id) => selected.has(id)).length
  return count && count === ids.length ? true : count ? ('indeterminate' as const) : false
}

interface SelectionProps {
  selected: Set<string>
  toggle(ids: string[], on: boolean): void
}

export function ProjectTable({
  groups,
  collapsed,
  toggleExpanded,
  selected,
  toggle,
}: SelectionProps & {
  groups: ProjectGroup[]
  collapsed: Set<string>
  toggleExpanded(id: string): void
}) {
  const s = useStore()
  const { t } = s
  const ids = groups
    .flatMap(groupWorkspaces)
    .flatMap(eligibleArtifacts)
    .map((a) => a.id)
  const busyReason =
    s.busy || s.plan
      ? t(
          '请先完成当前操作或关闭审阅窗口。',
          'Finish the current operation or close the review dialog first.',
        )
      : null
  return (
    <Card className="overflow-hidden py-0 shadow-none">
      <CardContent className="px-0">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead className="w-10 pl-4">
                <SelectionCheckbox
                  aria-label={t('选择所有可清理项目', 'Select all eligible artifacts')}
                  reason={
                    busyReason ||
                    (!ids.length
                      ? t('当前列表没有可清理目录。', 'No eligible directories in this list.')
                      : null)
                  }
                  checked={selectionState(ids, selected)}
                  onCheckedChange={(on) => toggle(ids, on === true)}
                />
              </TableHead>
              <TableHead>{t('项目', 'Project')}</TableHead>
              <TableHead>{t('最近活动', 'Last activity')}</TableHead>
              <TableHead>{t('可清理目录', 'Artifacts')}</TableHead>
              <TableHead className="text-right">{t('占用', 'Size')}</TableHead>
              <TableHead>
                <span className="sr-only">{t('操作', 'Actions')}</span>
              </TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {groups.map((group) => {
              const family = groupWorkspaces(group)
              const familyIds = family.flatMap(eligibleArtifacts).map((a) => a.id)
              const expanded = !collapsed.has(group.repository.id)
              return (
                <Fragment key={group.repository.id}>
                  <WorkspaceRow
                    node={{
                      id: group.root?.id ?? group.repository.id,
                      name: group.repository.name,
                      path: group.repository.path,
                      branch: group.root?.branch ?? null,
                      project: group.root,
                    }}
                    included={group.includeRoot}
                    ids={familyIds}
                    selected={selected}
                    toggle={toggle}
                    familySize={
                      group.children.length
                        ? family.reduce((sum, p) => sum + projectBytes(p), 0)
                        : undefined
                    }
                    disclosure={
                      group.children.length ? (
                        <Button
                          variant="ghost"
                          size="sm"
                          className="h-6 gap-1 px-1.5 text-xs font-normal"
                          aria-expanded={expanded}
                          aria-label={`${t('展开或收起工作树', 'Toggle worktrees')} · ${group.repository.name}`}
                          onClick={() => toggleExpanded(group.repository.id)}
                        >
                          <ChevronRight
                            className={`size-3 transition-transform ${expanded ? 'rotate-90' : ''}`}
                          />
                          <GitFork className="size-3" />
                          {group.children.length} {t('个 worktree', 'worktrees')}
                        </Button>
                      ) : undefined
                    }
                  />
                  {expanded &&
                    group.children.map((node, index) => (
                      <WorkspaceRow
                        key={node.id}
                        node={node}
                        included
                        ids={eligibleArtifacts(node.project).map((a) => a.id)}
                        child={index === group.children.length - 1 ? 'last' : 'branch'}
                        selected={selected}
                        toggle={toggle}
                      />
                    ))}
                </Fragment>
              )
            })}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  )
}

function WorkspaceRow({
  node,
  included,
  ids,
  familySize,
  disclosure,
  child,
  selected,
  toggle,
}: SelectionProps & {
  node: WorkspaceNode
  included: boolean
  ids: string[]
  familySize?: number
  disclosure?: React.ReactNode
  child?: 'branch' | 'last'
}) {
  const s = useStore()
  const { t } = s
  const project = node.project
  const worktree = node.worktree
  const busyReason =
    s.busy || s.plan
      ? t(
          '请先完成当前操作或关闭审阅窗口。',
          'Finish the current operation or close the review dialog first.',
        )
      : null
  const excluded = !included
    ? t(
        '仅展示仓库归属，不参与当前筛选的清理。',
        'Repository context only; excluded from cleanup by the current filter.',
      )
    : null
  const unavailable = !project
    ? node.worktree?.issue ||
      t(
        '此工作区尚未完成扫描，请检查路径、排除规则和权限后重新扫描。',
        'This workspace was not scanned. Check its path, exclusions, and permissions, then rescan.',
      )
    : project.protected
      ? t('项目已保护', 'Project is protected')
      : t('没有可安全清理的目录', 'No eligible directories')
  const removalReason = worktree?.locked
    ? t('工作树已锁定，请先通过 Git 解锁。', 'Worktree is locked. Unlock it through Git first.')
    : excluded ||
      (project?.protected ? t('项目已保护', 'Project is protected') : null) ||
      (!project ? unavailable : worktree?.issue) ||
      (!worktree?.size?.complete
        ? t('请先完成工作树扫描。', 'Complete the worktree scan first.')
        : null)
  const protect = async () => {
    if (!project) return
    const settings = s.data.settings
    const saved = await s.saveSettings({
      ...settings,
      protectedProjects: project.protected
        ? settings.protectedProjects.filter((p) => p !== project.path)
        : [...settings.protectedProjects, project.path],
    })
    if (saved) await s.refresh()
  }
  const checkbox = (
    <SelectionCheckbox
      aria-label={`${t('选择', 'Select')} ${node.name}`}
      reason={busyReason || (!ids.length ? excluded || unavailable : null)}
      checked={selectionState(ids, selected)}
      onCheckedChange={(on) => toggle(ids, on === true)}
    />
  )
  const identity = (
    <div className="min-w-0">
      <div className="flex flex-wrap items-center gap-2 font-medium">
        {child ? (
          <GitFork
            aria-label={t('关联工作树', 'Linked worktree')}
            className="size-3.5 text-muted-foreground"
          />
        ) : project?.providers[0] ? (
          <span title={metadata[project.providers[0]].name}>
            <EcoDot id={project.providers[0]} />
          </span>
        ) : (
          <FolderGit2
            aria-label={t('Git 仓库', 'Git repository')}
            className="size-4 text-muted-foreground"
          />
        )}
        <span>{node.name}</span>
        {project?.protected && (
          <Badge variant="outline" className="gap-1 text-xs">
            <ShieldCheck className="size-3" />
            {t('已保护', 'Protected')}
          </Badge>
        )}
        {node.worktree?.locked && (
          <Badge
            variant="outline"
            className="gap-1 text-xs"
            title={t(
              'Git 已锁定此工作树，可以清理产物，但不能移除工作树。',
              'This worktree is locked. Generated artifacts can be cleaned, but the worktree cannot be removed.',
            )}
          >
            <LockKeyhole className="size-3" />
            {t('已锁定', 'Locked')}
          </Badge>
        )}
        {!included && (
          <Badge variant="secondary" className="text-[10px] font-normal">
            {t('所属仓库', 'Repository')}
          </Badge>
        )}
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
        <span className="max-w-72 truncate font-mono" title={node.path}>
          {node.path}
        </span>
        <span className="inline-flex items-center gap-1">
          <GitBranch className="size-3" />
          {node.branch ?? t('分支未知', 'Unknown branch')}
        </span>
        {disclosure}
      </div>
      {project && project.providers.length > 1 && (
        <p className="mt-1 text-xs text-muted-foreground">
          {project.providers.map((id) => metadata[id].short).join(' · ')}
        </p>
      )}
    </div>
  )
  return (
    <TableRow className={child ? 'bg-muted/20' : undefined} data-workspace-id={node.id}>
      {child ? (
        <TableCell colSpan={2} className="relative pl-12">
          <span
            aria-hidden
            className={`absolute left-7 top-0 w-4 border-l border-border ${child === 'last' ? 'h-1/2 border-b' : 'h-full'}`}
          />
          {child === 'branch' && (
            <span aria-hidden className="absolute left-7 top-1/2 w-4 border-t border-border" />
          )}
          <div className="flex items-center gap-3 pl-1">
            {checkbox}
            {identity}
          </div>
        </TableCell>
      ) : (
        <>
          <TableCell className="pl-4">{checkbox}</TableCell>
          <TableCell>{identity}</TableCell>
        </>
      )}
      <TableCell className="text-xs text-muted-foreground">
        {project?.lastActive
          ? new Date(project.lastActive * 1000).toLocaleDateString(s.data.settings.language)
          : t('未知', 'Unknown')}
        {project && !project.activityComplete && <p>{t('扫描不完整', 'Partial scan')}</p>}
      </TableCell>
      <TableCell>
        <div className="flex min-w-28 max-w-72 flex-wrap gap-1.5">
          {project?.artifacts.map((artifact) => {
            const reason = excluded || cleanupReason(project, artifact, t)
            return (
              <label
                key={artifact.id}
                className={`inline-flex max-w-full items-center gap-1.5 rounded-md bg-muted px-2 py-1 text-xs ${reason ? 'text-muted-foreground' : 'cursor-pointer'}`}
                title={busyReason || reason ? undefined : artifact.path}
              >
                <SelectionCheckbox
                  className="size-3.5"
                  aria-label={relativePath(project, artifact)}
                  reason={busyReason || reason}
                  checked={!reason && selected.has(artifact.id)}
                  onCheckedChange={(on) => toggle([artifact.id], on === true)}
                />
                <span className="break-all whitespace-normal font-mono">
                  {relativePath(project, artifact)}
                </span>
              </label>
            )
          })}
          {!project?.artifacts.length && (
            <span className="text-xs text-muted-foreground">
              {project
                ? t('未发现可重建产物', 'No generated artifacts found')
                : t('未完成扫描', 'Not scanned')}
            </span>
          )}
        </div>
      </TableCell>
      <TableCell className="text-right font-mono text-xs">
        {worktree?.size ? (
          <>
            {!worktree.size.complete && '≥ '}
            {formatBytes(worktree.size.bytes)}
            <span className="ml-1 font-sans text-muted-foreground">{t('总计', 'Total')}</span>
            {project && (
              <p className="mt-1 whitespace-nowrap text-[10px] text-muted-foreground">
                {t('产物', 'Artifacts')} {formatBytes(projectBytes(project))}
              </p>
            )}
          </>
        ) : project ? (
          <>
            {project.artifacts.some((a) => !a.size.complete) && '≥ '}
            {formatBytes(projectBytes(project))}
          </>
        ) : (
          '—'
        )}
        {familySize !== undefined && (
          <p className="mt-1 whitespace-nowrap text-[10px] text-muted-foreground">
            {t('产物含 worktree', 'Artifacts with worktrees')} {formatBytes(familySize)}
          </p>
        )}
      </TableCell>
      <TableCell className="text-right">
        <div className="flex flex-col items-end gap-1">
          <ActionButton
            variant="ghost"
            size="sm"
            reason={busyReason || (!ids.length ? excluded || unavailable : null)}
            onClick={() => void s.prepare({ kind: 'cleanProjects', artifactIds: ids })}
          >
            <Trash2 />
            {familySize !== undefined
              ? t('全部清理产物', 'Clean group artifacts')
              : t('清理产物', 'Clean artifacts')}
          </ActionButton>
          {worktree && (
            <ActionButton
              variant="ghost"
              size="sm"
              className="text-xs text-muted-foreground hover:text-destructive"
              reason={busyReason || removalReason}
              onClick={() => void s.prepare({ kind: 'removeWorktree', id: worktree.id })}
            >
              {t('移除工作树', 'Remove worktree')}
            </ActionButton>
          )}
          {project && (
            <ActionButton
              variant="ghost"
              size="sm"
              reason={busyReason}
              title={t(
                '保护项目后，其产物不会进入清理计划。',
                'Protected project artifacts are excluded from cleanup plans.',
              )}
              className="text-xs text-muted-foreground"
              onClick={() => void protect()}
            >
              <ShieldCheck />
              {project.protected
                ? t('取消保护', 'Unprotect project')
                : t('保护项目', 'Protect project')}
            </ActionButton>
          )}
        </div>
      </TableCell>
    </TableRow>
  )
}
