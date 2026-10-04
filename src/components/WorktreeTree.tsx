import { FolderGit2, GitFork, LockKeyhole, RefreshCw } from 'lucide-react'
import type { Project, Worktree } from '@/domain'
import { useStore } from '@/store'
import { ProjectTable } from './ProjectTable'
import { ActionButton } from './action-controls'
import { Empty } from './shared'
import { Badge } from './ui/badge'
import { Button } from './ui/button'

export function WorktreeTree({
  projects,
  selected,
  toggle,
  query,
  showUnscanned,
}: {
  projects: Project[]
  selected: Set<string>
  toggle(ids: string[], on: boolean): void
  query: string
  showUnscanned: boolean
}) {
  const s = useStore()
  const { t } = s
  const inventory = s.data.inventory
  const scannedIds = new Set(inventory.projects.map((p) => p.id))
  const visibleIds = new Set(projects.map((p) => p.id))
  const entries = new Map((inventory.worktrees ?? []).map((w) => [w.id, w]))
  for (const p of inventory.projects) {
    if (p.isWorktree && p.repository && !entries.has(p.id)) {
      entries.set(p.id, {
        id: p.id,
        path: p.path,
        branch: p.branch,
        repository: p.repository,
        locked: false,
        issue: null,
      })
    }
  }
  const groups = new Map<string, { repository: Worktree['repository']; entries: Worktree[] }>()
  for (const w of entries.values()) {
    const visible = scannedIds.has(w.id)
      ? visibleIds.has(w.id)
      : showUnscanned &&
        [w.repository.name, w.path, w.branch]
          .join(' ')
          .toLowerCase()
          .includes(query.trim().toLowerCase())
    if (!visible) continue
    const group = groups.get(w.repository.id) ?? { repository: w.repository, entries: [] }
    group.entries.push(w)
    groups.set(w.repository.id, group)
  }
  if (!groups.size)
    return (
      <Empty>
        {t(
          '未找到符合条件的 Worktree。扫描包含 Git 仓库的目录后会显示关联工作树。',
          'No matching worktrees. Scan a folder containing Git repositories to discover their linked worktrees.',
        )}
      </Empty>
    )
  return (
    <div className="space-y-4">
      {[...groups.values()].map(({ repository, entries }) => {
        const children = projects.filter((p) => p.repository?.id === repository.id)
        return (
          <details key={repository.id} open className="rounded-xl border p-4">
            <summary className="cursor-pointer text-sm font-medium">
              <span className="ml-1 inline-flex items-center gap-2">
                <FolderGit2 className="size-4" />
                {repository.name}
                <Badge variant="secondary">{entries.length}</Badge>
              </span>
              <p className="mt-1 break-all pl-5 font-mono text-xs font-normal text-muted-foreground">
                {repository.path}
              </p>
            </summary>
            <div className="mt-4 space-y-3 border-l-2 border-muted pl-4">
              {children.length > 0 && (
                <ProjectTable
                  projects={children}
                  selected={selected}
                  toggle={toggle}
                  label={repository.name}
                />
              )}
              {entries
                .filter((w) => !scannedIds.has(w.id))
                .map((w) => (
                  <div key={w.id} className="rounded-lg border border-dashed bg-muted/20 p-3">
                    <div className="flex items-center gap-2 text-sm font-medium">
                      <GitFork className="size-4" />
                      {w.branch ?? t('分支未知', 'Unknown branch')}
                      {w.locked && (
                        <Badge variant="outline">
                          <LockKeyhole className="size-3" />
                          {t('锁定', 'Locked')}
                        </Badge>
                      )}
                    </div>
                    <p className="mt-1 break-all font-mono text-xs text-muted-foreground">
                      {w.path}
                    </p>
                    <p className="my-2 text-xs text-muted-foreground">
                      {w.issue ||
                        t(
                          '主仓库在扫描范围时会自动扫描关联工作树。此路径未完成扫描，请检查排除规则、路径和权限。',
                          'Linked worktrees are scanned automatically with their main repository. This path was not scanned; check exclusions, its location, and permissions.',
                        )}
                    </p>
                    <ActionButton
                      size="sm"
                      variant="outline"
                      reason={
                        s.busy
                          ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
                          : null
                      }
                      onClick={() => void s.refresh()}
                    >
                      <RefreshCw />
                      {t('重新扫描', 'Rescan')}
                    </ActionButton>
                    <Button size="sm" variant="ghost" onClick={() => s.go('settings')}>
                      {t('扫描设置', 'Scan settings')}
                    </Button>
                  </div>
                ))}
            </div>
          </details>
        )
      })}
    </div>
  )
}
