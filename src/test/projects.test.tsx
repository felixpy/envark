import { describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StoreProvider } from '@/store'
import Projects from '@/views/Projects'
import { emptySnapshot, type Project, type Snapshot } from '@/domain'
import type { Backend } from '@/bridge'
import { TooltipProvider } from '@/components/ui/tooltip'

function project(id: string, protectedProject: boolean, ageDays: number): Project {
  return {
    id,
    name: id,
    path: `/projects/${id}`,
    providers: ['js'],
    lastActive: Date.now() / 1000 - ageDays * 86400,
    activityComplete: true,
    branch: 'main',
    pins: {},
    protected: protectedProject,
    artifacts: [
      {
        id: `${id}-modules`,
        name: 'node_modules',
        path: `/projects/${id}/node_modules`,
        kind: 'dependencies',
        size: { bytes: 1024, files: 1, skipped: 0, complete: true },
        restore: 'pnpm install',
        canClean: true,
        cleanupIssue: null,
      },
    ],
  }
}

function fixture(configure?: (data: Snapshot) => void) {
  const data: Snapshot = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.roots = ['/projects']
  data.settings.idleDays = 30
  data.inventory.projects = [
    project('older', false, 45),
    project('recent', false, 5),
    project('protected', true, 60),
    project('unverified', false, 60),
  ]
  data.inventory.projects[3].artifacts[0].canClean = false
  data.inventory.projects[3].artifacts[0].cleanupIssue = 'Ownership is unverified.'
  configure?.(data)
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'clean',
    createdAt: Date.now() / 1000,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  const api: Backend = {
    native: false,
    snapshot: async () => data,
    refresh: async () => data,
    saveSettings: async (settings) => ({ ...data, settings }),
    prepare,
    execute: vi.fn(),
    cancel: vi.fn(),
    selectFolder: vi.fn(),
    readConfig: vi.fn(),
    saveConfig: vi.fn(),
    subscribe: async () => () => {},
  }
  render(
    <StoreProvider api={api}>
      <Projects />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  return { prepare }
}

describe('project cleanup selection', () => {
  it('excludes protected projects from bulk cleanup and passes only artifact IDs', async () => {
    const { prepare } = fixture()
    const user = userEvent.setup()
    await screen.findByText('older')
    await user.click(screen.getByRole('checkbox', { name: 'Select all eligible artifacts' }))
    await user.click(screen.getByRole('button', { name: 'Review cleanup' }))
    await waitFor(() =>
      expect(prepare).toHaveBeenCalledWith({
        kind: 'cleanProjects',
        artifactIds: ['recent-modules', 'older-modules'],
      }),
    )
    expect(
      screen.getByRole('checkbox', { name: 'Select protected' }).getAttribute('aria-disabled'),
    ).toBe('true')
    expect(
      screen.getByRole('checkbox', { name: 'Select unverified' }).getAttribute('aria-disabled'),
    ).toBe('true')
    expect(screen.queryByText('Ownership is unverified.')).toBeNull()
    expect(
      screen.getByRole('checkbox', { name: 'Select unverified' }).getAttribute('aria-description'),
    ).toBeTruthy()
  })
  it('uses the saved inactivity threshold, not the prototype’s fixed 90 days', async () => {
    fixture()
    const user = userEvent.setup()
    await screen.findByText('older')
    await user.click(screen.getByRole('switch'))
    expect(screen.queryByText('recent')).toBeNull()
    expect(screen.getByText('older')).toBeTruthy()
  })
})

function withWorktrees(data: Snapshot) {
  const root = project('repository', false, 2)
  const repository = { id: 'repo', name: root.name, path: root.path }
  root.repository = repository
  const child = (id: string, days: number, protectedProject = false) => ({
    ...project(id, protectedProject, days),
    path: `/outside/${id}`,
    repository,
    isWorktree: true,
  })
  data.inventory.projects = [
    root,
    child('old-branch', 120),
    child('active-branch', 1),
    child('protected-branch', 140, true),
  ]
  data.inventory.worktrees = data.inventory.projects
    .filter((p) => p.isWorktree)
    .map((p) => ({
      id: p.id,
      repository,
      path: p.path,
      branch: p.name,
      locked: p.protected,
      issue: null,
    }))
}

it('selects a repository family, keeps partial state, and retains collapsed selections', async () => {
  const { prepare } = fixture(withWorktrees)
  const user = userEvent.setup()
  await screen.findByText('repository')
  await user.click(screen.getByRole('checkbox', { name: 'Select old-branch' }))
  expect(
    screen.getByRole('checkbox', { name: 'Select repository' }).getAttribute('aria-checked'),
  ).toBe('mixed')
  await user.click(screen.getByRole('checkbox', { name: 'Select repository' }))
  await user.click(screen.getByRole('button', { name: 'Collapse worktrees' }))
  expect(screen.queryByText('old-branch')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Review cleanup' }))
  expect(prepare).toHaveBeenCalledWith({
    kind: 'cleanProjects',
    artifactIds: ['repository-modules', 'active-branch-modules', 'old-branch-modules'],
  })
})

it('keeps the parent as context when filtering a worktree and never cleans active siblings', async () => {
  const { prepare } = fixture(withWorktrees)
  const user = userEvent.setup()
  await screen.findByText('repository')
  await user.click(screen.getByRole('switch'))
  expect(screen.queryByText('active-branch')).toBeNull()
  await user.type(
    screen.getByPlaceholderText('Search repositories, branches, or paths'),
    'old-branch',
  )
  expect(screen.getByText('repository')).toBeTruthy()
  expect(screen.queryByText('protected-branch')).toBeNull()
  await user.click(screen.getByRole('checkbox', { name: 'Select repository' }))
  await user.click(screen.getByRole('button', { name: 'Review cleanup' }))
  expect(prepare).toHaveBeenCalledWith({
    kind: 'cleanProjects',
    artifactIds: ['old-branch-modules'],
  })
})

it('keeps missing worktree reasons inside an on-demand hint and prevents cleanup', async () => {
  fixture((data) => {
    withWorktrees(data)
    data.inventory.worktrees.push({
      id: 'missing',
      repository: data.inventory.worktrees[0].repository,
      path: '/outside/missing',
      branch: 'missing',
      locked: false,
      issue: 'Worktree needs repair.',
    })
  })
  const control = await screen.findByRole('checkbox', { name: 'Select missing' })
  expect(control.getAttribute('aria-disabled')).toBe('true')
  expect(screen.queryByText('Worktree needs repair.')).toBeNull()
  await userEvent.setup().hover(control)
  expect((await screen.findByRole('tooltip')).textContent).toBe('Worktree needs repair.')
})

it('sorts by activity and total size while keeping worktrees with their repository', async () => {
  fixture((data) => {
    withWorktrees(data)
    const other = project('other', false, 5)
    other.artifacts[0].size.bytes = 2048
    const unknown = project('unknown', false, 0)
    unknown.lastActive = null
    data.inventory.projects.push(other, unknown)
  })
  const user = userEvent.setup()
  await screen.findByText('repository')
  const order = () =>
    screen
      .getAllByRole('row')
      .map((row) => row.getAttribute('data-workspace-id'))
      .filter(Boolean)
  expect(order()).toEqual([
    'repository',
    'active-branch',
    'old-branch',
    'protected-branch',
    'other',
    'unknown',
  ])
  await user.click(screen.getByRole('combobox', { name: 'Sort projects' }))
  await user.click(screen.getByRole('option', { name: 'Activity: oldest first' }))
  expect(order()).toEqual([
    'other',
    'repository',
    'protected-branch',
    'old-branch',
    'active-branch',
    'unknown',
  ])
  await user.click(screen.getByRole('combobox', { name: 'Sort projects' }))
  await user.click(screen.getByRole('option', { name: 'Size: largest first' }))
  expect(order()[0]).toBe('repository')
  await user.click(screen.getByRole('combobox', { name: 'Sort projects' }))
  await user.click(screen.getByRole('option', { name: 'Size: smallest first' }))
  expect(order().slice(0, 3)).toEqual(['unknown', 'other', 'repository'])
})

it('shows whole checkout size and sends a separate worktree removal request', async () => {
  const { prepare } = fixture((data) => {
    withWorktrees(data)
    for (const worktree of data.inventory.worktrees) {
      worktree.size = { bytes: 1048576, files: 12, skipped: 0, complete: true }
    }
  })
  const name = await screen.findByRole('checkbox', { name: 'Select old-branch' })
  const row = within(name.closest('tr')!)
  expect(row.getByText('1.0 MB')).toBeTruthy()
  expect(row.getByText('Total')).toBeTruthy()
  expect(row.getByText('Artifacts 1.0 KB')).toBeTruthy()
  await userEvent.setup().click(row.getByRole('button', { name: 'Remove worktree' }))
  expect(prepare).toHaveBeenCalledWith({ kind: 'removeWorktree', id: 'old-branch' })
})

it('blocks locked worktree removal using an on-demand reason while allowing artifact cleanup', async () => {
  fixture((data) => {
    withWorktrees(data)
    data.inventory.projects[1].protected = false
    data.inventory.worktrees[0].locked = true
    data.inventory.worktrees[0].size = { bytes: 2048, files: 2, skipped: 0, complete: true }
  })
  const name = await screen.findByRole('checkbox', { name: 'Select old-branch' })
  const row = within(name.closest('tr')!)
  const remove = row.getByRole('button', { name: 'Remove worktree' })
  expect(remove.getAttribute('aria-disabled')).toBe('true')
  expect(
    row.getByRole('button', { name: 'Clean artifacts' }).getAttribute('aria-disabled'),
  ).toBeNull()
  expect(screen.queryByText('Worktree is locked. Unlock it through Git first.')).toBeNull()
  await userEvent.setup().hover(remove)
  expect((await screen.findByRole('tooltip')).textContent).toBe(
    'Worktree is locked. Unlock it through Git first.',
  )
})
