import { displayPath } from '@/lib/paths'
import { useState } from 'react'
import {
  ArrowDownWideNarrow,
  ChevronsDownUp,
  ChevronsUpDown,
  FolderPlus,
  RefreshCw,
  Trash2,
} from 'lucide-react'
import { formatBytes, metadata } from '@/domain'
import { useStore } from '@/store'
import { useSelection } from '@/hooks/use-selection'
import {
  eligibleArtifacts,
  eligibleWorktrees,
  type ProjectMode,
  filterProjectGroups,
  groupProjects,
  groupWorkspaces,
  type ProjectSort,
} from '@/lib/project-tree'
import { Empty, PageHeader, SearchInput } from '@/components/shared'
import { ActionButton } from '@/components/action-controls'
import { ProjectTable } from '@/components/ProjectTable'
import { Badge } from '@/components/ui/badge'
import { Button } from '@/components/ui/button'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select'

export default function Projects() {
  const s = useStore()
  const { t } = s
  const { inventory, settings } = s.data
  const [mode, setMode] = useState<ProjectMode>('artifacts')
  const [cleanableOnly, setCleanableOnly] = useState(true)
  const [query, setQuery] = useState('')
  const [provider, setProvider] = useState('all')
  const [idleOnly, setIdleOnly] = useState(s.focus.filter === 'idle')
  const [sort, setSort] = useState<ProjectSort>('activity-desc')
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const busyReason =
    s.busy || s.plan
      ? t(
          '请先完成当前操作或关闭审阅窗口。',
          'Finish the current operation or close the review dialog first.',
        )
      : null
  const groups = filterProjectGroups(groupProjects(inventory), {
    query,
    provider,
    idleOnly,
    idleDays: settings.idleDays,
    sort,
    mode,
    cleanableOnly,
  })
  const projects = groups.flatMap(groupWorkspaces)
  const eligible = projects.flatMap(eligibleArtifacts)
  const artifactSelection = useSelection(eligible.map((a) => a.id))
  const worktrees = groups.flatMap(eligibleWorktrees)
  const worktreeSelection = useSelection(worktrees.map((w) => w.id))
  const selection = mode === 'worktrees' ? worktreeSelection : artifactSelection
  const chosenWorktrees = worktrees.filter((w) => worktreeSelection.selected.has(w.id))
  const chosen = eligible.filter((a) => selection.selected.has(a.id))
  const selectedProjects = projects.filter((p) =>
    eligibleArtifacts(p).some((a) => selection.selected.has(a.id)),
  )
  const selectedWorktrees = selectedProjects.filter((p) => p.isWorktree).length
  const parents = groups.filter((g) => g.children.length)
  const allExpanded = parents.length > 0 && parents.every((g) => !collapsed.has(g.repository.id))
  const toggleExpanded = (id: string) =>
    setCollapsed((current) => {
      const next = new Set(current)
      if (next.has(id)) next.delete(id)
      else next.add(id)
      return next
    })
  return (
    <div className="space-y-6">
      <PageHeader
        title={t('项目空间', 'Project space')}
        description={t(
          '按 Git 仓库汇总，自动包含关联 worktree，查看产物与工作树整体占用。',
          'Git repositories and linked worktrees, with generated artifacts and total checkout sizes.',
        )}
        actions={
          <>
            <ActionButton variant="outline" reason={busyReason} onClick={() => void s.addRoot()}>
              <FolderPlus />
              {t('添加目录', 'Add folder')}
            </ActionButton>
            <ActionButton variant="outline" reason={busyReason} onClick={() => void s.refresh()}>
              <RefreshCw />
              {t('重新扫描', 'Rescan')}
            </ActionButton>
          </>
        }
      />
      <Tabs value={mode} onValueChange={(value) => setMode(value as ProjectMode)}>
        <TabsList>
          <TabsTrigger value="artifacts">{t('产物与依赖', 'Artifacts & dependencies')}</TabsTrigger>
          <TabsTrigger value="worktrees">{t('工作树', 'Worktrees')}</TabsTrigger>
        </TabsList>
      </Tabs>
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-3">
          <SearchInput
            value={query}
            onChange={setQuery}
            placeholder={t('搜索仓库、分支或路径', 'Search repositories, branches, or paths')}
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
          {mode === 'worktrees' && parents.length > 0 && (
            <Button
              variant="outline"
              size="sm"
              onClick={() =>
                setCollapsed((current) => {
                  const next = new Set(current)
                  for (const group of parents) {
                    if (allExpanded) next.add(group.repository.id)
                    else next.delete(group.repository.id)
                  }
                  return next
                })
              }
            >
              {allExpanded ? <ChevronsDownUp /> : <ChevronsUpDown />}
              {allExpanded
                ? t('收起 worktree', 'Collapse worktrees')
                : t('展开 worktree', 'Expand worktrees')}
            </Button>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-4">
          <Select value={sort} onValueChange={(value) => setSort(value as ProjectSort)}>
            <SelectTrigger aria-label={t('项目排序', 'Sort projects')} className="w-auto min-w-40">
              <ArrowDownWideNarrow className="size-4" />
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="activity-desc">
                {t('活动：最近优先', 'Activity: newest first')}
              </SelectItem>
              <SelectItem value="activity-asc">
                {t('活动：最早优先', 'Activity: oldest first')}
              </SelectItem>
              <SelectItem value="size-desc">
                {t('占用：从大到小', 'Size: largest first')}
              </SelectItem>
              <SelectItem value="size-asc">
                {t('占用：从小到大', 'Size: smallest first')}
              </SelectItem>
            </SelectContent>
          </Select>
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <Switch
              aria-label={t('仅闲置', 'Idle only')}
              checked={idleOnly}
              onCheckedChange={setIdleOnly}
            />
            {t('仅闲置', 'Idle only')} &gt; {settings.idleDays} {t('天', 'days')}
          </label>
          <label className="flex items-center gap-2 text-sm text-muted-foreground">
            <Switch
              aria-label={t('仅可清理', 'Cleanable only')}
              checked={cleanableOnly}
              onCheckedChange={setCleanableOnly}
            />
            {t('仅可清理', 'Cleanable only')}
          </label>
        </div>
      </div>
      {groups.length ? (
        <ProjectTable
          mode={mode}
          groups={groups}
          collapsed={collapsed}
          toggleExpanded={toggleExpanded}
          selected={selection.selected}
          toggle={selection.toggle}
        />
      ) : (
        <Empty>
          {settings.roots.length
            ? t(
                '没有符合条件的 Git 仓库或 Worktree。请检查筛选条件和扫描范围，或重新扫描。',
                'No matching Git repositories or worktrees. Check filters and scan roots, or rescan.',
              )
            : t(
                '添加存放 Git 仓库的目录后开始扫描。',
                'Add a folder containing Git repositories to start scanning.',
              )}
        </Empty>
      )}
      {(mode === 'worktrees' ? chosenWorktrees.length > 0 : chosen.length > 0) && (
        <div className="sticky bottom-4 z-10 mx-auto flex w-fit max-w-full flex-wrap items-center gap-4 rounded-xl border bg-background px-4 py-3 shadow-lg">
          {mode === 'worktrees' ? (
            <span className="text-sm">
              {t('已选', 'Selected')} {chosenWorktrees.length} {t('个工作树', 'worktrees')} ·{' '}
              <span className="font-mono">
                {formatBytes(
                  chosenWorktrees.reduce((sum, w) => sum + (w.worktree?.size?.bytes ?? 0), 0),
                )}
              </span>
            </span>
          ) : (
            <span className="text-sm">
              {t('已选', 'Selected')} {selectedProjects.length} {t('个工作区', 'workspaces')}
              {selectedWorktrees > 0 && (
                <>
                  {' '}
                  ({t('含', 'including')} {selectedWorktrees} worktree)
                </>
              )}{' '}
              · {chosen.length} {t('个目录', 'directories')} ·{' '}
              <span className="font-mono">
                {formatBytes(chosen.reduce((sum, a) => sum + a.size.bytes, 0))}
              </span>
            </span>
          )}
          <Button
            variant="ghost"
            size="sm"
            onClick={() => selection.toggle([...selection.selected], false)}
          >
            {t('取消', 'Cancel')}
          </Button>
          <ActionButton
            reason={busyReason}
            onClick={() =>
              void s.prepare(
                mode === 'worktrees'
                  ? { kind: 'removeWorktrees', ids: chosenWorktrees.map((w) => w.id) }
                  : { kind: 'cleanProjects', artifactIds: chosen.map((a) => a.id) },
              )
            }
          >
            <Trash2 />
            {mode === 'worktrees'
              ? t('审阅移除工作树', 'Review worktree removal')
              : t('审阅清理', 'Review cleanup')}
          </ActionButton>
        </div>
      )}
      <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground">
        <span>{t('扫描范围', 'Scan roots')}:</span>
        {settings.roots.map((root) => (
          <Badge key={displayPath(root)} variant="outline" className="font-mono font-normal">
            {displayPath(root)}
          </Badge>
        ))}
      </div>
    </div>
  )
}
