import {
  idle,
  projectBytes,
  type Artifact,
  type Inventory,
  type Project,
  type ProviderId,
  type Repository,
  type Worktree,
} from '@/domain'

export interface WorkspaceNode {
  id: string
  name: string
  path: string
  branch: string | null
  project?: Project
  worktree?: Worktree
}

export interface ProjectGroup {
  repository: Repository
  root?: Project
  includeRoot: boolean
  children: WorkspaceNode[]
}

export type ProjectSort = 'activity-desc' | 'activity-asc' | 'size-desc' | 'size-asc'

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

export function eligibleArtifacts(project?: Project) {
  return (
    project?.artifacts.filter((artifact) => !cleanupReason(project, artifact, (_, en) => en)) ?? []
  )
}

export function groupProjects(inventory: Inventory): ProjectGroup[] {
  const groups = new Map<string, ProjectGroup>()
  for (const project of inventory.projects.filter((p) => !p.isWorktree)) {
    const repository = project.repository ?? {
      id: project.id,
      name: project.name,
      path: project.path,
    }
    groups.set(repository.id, { repository, root: project, includeRoot: true, children: [] })
  }
  const projects = new Map(inventory.projects.map((p) => [p.id, p]))
  const worktrees = new Map((inventory.worktrees ?? []).map((w) => [w.id, w]))
  for (const project of inventory.projects) {
    if (project.isWorktree && project.repository && !worktrees.has(project.id)) {
      worktrees.set(project.id, {
        id: project.id,
        path: project.path,
        branch: project.branch,
        repository: project.repository,
        locked: false,
        issue: null,
      })
    }
  }
  for (const worktree of worktrees.values()) {
    const group = groups.get(worktree.repository.id) ?? {
      repository: worktree.repository,
      includeRoot: false,
      children: [],
    }
    const project = projects.get(worktree.id)
    group.children.push({
      id: worktree.id,
      name:
        project?.name ??
        worktree.path.replaceAll('\\', '/').split('/').filter(Boolean).at(-1) ??
        worktree.path,
      path: worktree.path,
      branch: worktree.branch,
      project,
      worktree,
    })
    groups.set(worktree.repository.id, group)
  }
  return [...groups.values()]
}

export function groupWorkspaces(group: ProjectGroup) {
  return [
    ...(group.includeRoot && group.root ? [group.root] : []),
    ...group.children.flatMap((child) => (child.project ? [child.project] : [])),
  ]
}

export function filterProjectGroups(
  groups: ProjectGroup[],
  options: {
    query: string
    provider: string
    idleOnly: boolean
    idleDays: number
    sort: ProjectSort
  },
): ProjectGroup[] {
  const keyword = options.query.trim().toLowerCase()
  const matchesText = (node: { name: string; path: string; branch?: string | null }) =>
    [node.name, node.path, node.branch].join(' ').toLowerCase().includes(keyword)
  const matchesFilter = (project?: Project) =>
    (options.provider === 'all' || project?.providers.includes(options.provider as ProviderId)) &&
    (!options.idleOnly || (project && idle(project, options.idleDays)))
  const result = groups.flatMap((group) => {
    const parentMatches = matchesText(group.root ?? group.repository)
    const includeRoot = Boolean(group.root && parentMatches && matchesFilter(group.root))
    const children = group.children.filter(
      (child) => matchesFilter(child.project) && (parentMatches || matchesText(child)),
    )
    return includeRoot || children.length ? [{ ...group, includeRoot, children }] : []
  })
  const direction = options.sort.endsWith('asc') ? 1 : -1
  const bySize = options.sort.startsWith('size')
  const compare = (a: number | null, b: number | null, nameA: string, nameB: string) => {
    if (a === null && b !== null) return 1
    if (b === null && a !== null) return -1
    return a !== null && b !== null && a !== b ? (a - b) * direction : nameA.localeCompare(nameB)
  }
  const value = (projects: Project[]) => {
    if (!projects.length) return null
    if (bySize) return projects.reduce((sum, p) => sum + projectBytes(p), 0)
    const times = projects.flatMap((p) => (p.lastActive === null ? [] : [p.lastActive]))
    return times.length ? Math.max(...times) : null
  }
  for (const group of result) {
    group.children.sort((a, b) =>
      compare(
        a.project ? value([a.project]) : null,
        b.project ? value([b.project]) : null,
        a.name,
        b.name,
      ),
    )
  }
  return result.sort((a, b) =>
    compare(
      value(groupWorkspaces(a)),
      value(groupWorkspaces(b)),
      a.repository.path,
      b.repository.path,
    ),
  )
}
