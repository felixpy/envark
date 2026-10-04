import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import App from '@/App'
import { backend, type Backend } from '@/bridge'
import { emptySnapshot, emptyProvider, type Project } from '@/domain'

function fixture(native = false) {
  const data = structuredClone(emptySnapshot)
  data.platform = 'windows'
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.roots = ['/repos']
  data.inventory.scannedAt = Date.now() / 1000
  const repository = { id: 'repo', name: 'main-repo', path: '/repos/main-repo' }
  const project = (id: string, old: boolean, isWorktree = false): Project => ({
    id,
    name: id,
    path: isWorktree ? `/elsewhere/${id}` : `/repos/${id}`,
    providers: ['js'],
    lastActive: Date.now() / 1000 - (old ? 120 : 1) * 86400,
    activityComplete: true,
    branch: id,
    pins: {},
    protected: false,
    repository,
    isWorktree,
    artifacts: [
      {
        id: `${id}-artifact`,
        name: 'node_modules',
        path: isWorktree ? `/elsewhere/${id}/node_modules` : `/repos/${id}/node_modules`,
        kind: 'dependencies',
        size: { bytes: 4096, files: 1, skipped: 0, complete: true },
        restore: 'pnpm install',
        canClean: true,
        cleanupIssue: null,
      },
    ],
  })
  data.inventory.projects = [
    project('main-repo', true),
    project('recent-repo', false),
    project('feature', true, true),
  ]
  data.inventory.worktrees = [
    {
      id: 'feature',
      path: '/elsewhere/feature',
      repository,
      branch: 'feature',
      locked: false,
      issue: null,
    },
  ]
  const js = emptyProvider('js')
  js.detected = true
  data.inventory.providers = [js, emptyProvider('py')]
  let menu: ((action: string) => void) | undefined
  const refresh = vi.fn(async () => data)
  const api: Backend = {
    ...backend,
    native,
    snapshot: async () => data,
    refresh,
    subscribeMenu: async (handler) => {
      menu = handler
      return () => {
        menu = undefined
      }
    },
  }
  render(<App api={api} />)
  return { refresh, menu: (action: string) => act(() => menu?.(action)) }
}

it('overview cards and suggested rows open the correct destinations with idle filters', async () => {
  fixture()
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'Available updates' }))
  await screen.findByRole('heading', { name: 'Settings' })
  await user.click(screen.getByRole('button', { name: 'Overview' }))
  await user.click(await screen.findByRole('button', { name: /1 inactive projects/ }))
  await screen.findByRole('heading', { name: 'Project space' })
  expect(screen.getByRole('switch').getAttribute('aria-checked')).toBe('true')
  expect(screen.queryByText('recent-repo')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Overview' }))
  await user.click(await screen.findByRole('button', { name: /Worktree artifacts/ }))
  await screen.findByRole('heading', { name: 'Worktrees' })
  const main = screen.getByRole('main')
  expect(within(main).getByText('main-repo')).toBeTruthy()
  expect(within(main).getByTitle('/elsewhere/feature')).toBeTruthy()
  expect(within(main).queryByText('recent-repo')).toBeNull()
})

it('keeps missing ecosystems inspectable and removes capability labels from the catalog', async () => {
  fixture()
  const user = userEvent.setup()
  await screen.findByRole('heading', { name: 'Overview' })
  await user.click(screen.getByRole('button', { name: 'Environments & tools' }))
  await screen.findByRole('heading', { name: 'Environments & tools' })
  const main = screen.getByRole('main')
  const python = within(main).getByRole('button', { name: /Python.*Not detected/ })
  expect(python.className).toContain('bg-muted')
  expect(within(main).queryByText('Runtimes')).toBeNull()
  await user.click(python)
  await screen.findByRole('heading', { name: 'Python' })
  expect(
    screen.getByRole('button', { name: 'Install version' }).getAttribute('aria-description'),
  ).toMatch(/version manager/)
})

it('routes native menu events to navigation, help, and rescan', async () => {
  const { menu, refresh } = fixture()
  await screen.findByRole('heading', { name: 'Overview' })
  menu('worktrees')
  await screen.findByRole('heading', { name: 'Worktrees' })
  menu('help')
  expect(await screen.findByRole('dialog')).toBeTruthy()
  expect(screen.getByText('Getting started & shortcuts')).toBeTruthy()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Close' }))
  menu('refresh')
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
  menu('unrecognized-command')
  expect(refresh).toHaveBeenCalledOnce()
})

it('handles Windows shortcuts delivered to the webview without repeating operations', async () => {
  const { refresh } = fixture(true)
  await screen.findByRole('heading', { name: 'Overview' })
  fireEvent.keyDown(window, { key: '4', ctrlKey: true })
  await screen.findByRole('heading', { name: 'Worktrees' })
  fireEvent.keyDown(window, { key: 'F1' })
  expect(await screen.findByRole('dialog')).toBeTruthy()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Close' }))
  fireEvent.keyDown(window, { key: 'r', ctrlKey: true, repeat: true })
  expect(refresh).not.toHaveBeenCalled()
  fireEvent.keyDown(window, { key: 'r', ctrlKey: true })
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
})
