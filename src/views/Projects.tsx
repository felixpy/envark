import { useState } from 'react'
import { FolderPlus, RefreshCw, Trash2 } from 'lucide-react'
import { formatBytes, idle, metadata, type ProviderId } from '@/domain'
import { useStore } from '@/store'
import { useSelection } from '@/hooks/use-selection'
import { Empty, PageHeader, SearchInput } from '@/components/shared'
import { ActionButton, SelectionCheckbox } from '@/components/action-controls'
import { ProjectTable, cleanupReason } from '@/components/ProjectTable'
import { WorktreeTree } from '@/components/WorktreeTree'
import { Badge } from '@/components/ui/badge'
import { Switch } from '@/components/ui/switch'
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs'

export default function Projects({ worktrees = false }: { worktrees?: boolean }) {
  const s = useStore()
  const { t } = s
  const { inventory, settings } = s.data
  const [query, setQuery] = useState('')
  const [provider, setProvider] = useState('all')
  const [idleOnly, setIdleOnly] = useState(s.focus.filter === 'idle')
  const busyReason = s.busy
    ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
    : null
  const list = inventory.projects.filter(
    (p) =>
      Boolean(p.isWorktree) === worktrees &&
      (provider === 'all' || p.providers.includes(provider as ProviderId)) &&
      (!idleOnly || idle(p, settings.idleDays)) &&
      [p.name, p.path, p.branch, p.repository?.name]
        .join(' ')
        .toLowerCase()
        .includes(query.trim().toLowerCase()),
  )
  const eligible = list.flatMap((p) => p.artifacts.filter((a) => !cleanupReason(p, a, t)))
  const selection = useSelection(eligible.map((a) => a.id))
  const chosen = eligible.filter((a) => selection.selected.has(a.id))
  return (
    <div className="space-y-6">
      <PageHeader
        title={worktrees ? 'Worktrees' : t('项目空间', 'Project space')}
        description={
          worktrees
            ? t(
                '主仓库在扫描范围内即可自动发现关联工作树，无需逐个添加。清理产物会保留工作树与分支。',
                'Linked worktrees are included automatically when their main repository is in scope. Cleanup preserves worktrees and branches.',
              )
            : t(
                '按 Git 仓库根目录汇总，子项目的依赖和构建产物归入所属仓库。',
                'One entry per Git repository, including dependencies and build artifacts from its subprojects.',
              )
        }
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
      <div className="flex flex-wrap items-center gap-3">
        <SearchInput
          value={query}
          onChange={setQuery}
          placeholder={
            worktrees
              ? t('搜索仓库、分支或路径', 'Search repositories, branches, or paths')
              : t('搜索项目', 'Search projects')
          }
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
      <p role="status" className="text-xs text-muted-foreground">
        {eligible.length}{' '}
        {t(
          '个目录可选；受保护、归属不明或扫描不完整的目录不可清理。',
          'directories selectable. Protected, unverified, or partially scanned directories cannot be cleaned.',
        )}
      </p>
      {worktrees && (
        <label className="flex items-center gap-2 text-xs">
          <SelectionCheckbox
            aria-label={t(
              '选择全部工作树的可清理目录',
              'Select eligible artifacts across all worktrees',
            )}
            reason={
              busyReason ||
              (!eligible.length
                ? t('当前没有可清理目录。', 'No eligible directories in this view.')
                : null)
            }
            checked={selection.checked ? true : selection.chosen.length ? 'indeterminate' : false}
            onCheckedChange={(on) => selection.toggleAll(on === true)}
          />
          {t('选择全部工作树的可清理目录', 'Select eligible artifacts across all worktrees')}
        </label>
      )}
      {worktrees ? (
        <WorktreeTree
          projects={list}
          selected={selection.selected}
          toggle={selection.toggle}
          query={query}
          showUnscanned={provider === 'all' && !idleOnly}
        />
      ) : (
        <ProjectTable projects={list} selected={selection.selected} toggle={selection.toggle} />
      )}
      {!worktrees && !list.length && (
        <Empty>
          {settings.roots.length
            ? t(
                '没有符合条件的 Git 仓库。请检查筛选条件和扫描范围，或重新扫描。',
                'No matching Git repositories. Check filters and scan roots, or rescan.',
              )
            : t(
                '添加存放 Git 仓库的目录后开始扫描。',
                'Add a folder containing Git repositories to start scanning.',
              )}
        </Empty>
      )}
      {chosen.length > 0 && (
        <div className="sticky bottom-4 flex items-center justify-between rounded-xl border bg-background p-4 shadow-lg">
          <span className="text-sm">
            {chosen.length} {t('个目录', 'directories')} ·{' '}
            <span className="font-mono">
              {formatBytes(chosen.reduce((sum, a) => sum + a.size.bytes, 0))}
            </span>
          </span>
          <ActionButton
            reason={busyReason}
            onClick={() =>
              void s.prepare({ kind: 'cleanProjects', artifactIds: chosen.map((a) => a.id) })
            }
          >
            <Trash2 />
            {t('审阅清理', 'Review cleanup')}
          </ActionButton>
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
