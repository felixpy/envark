import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import App from '@/App'
import { backend, type Backend } from '@/bridge'
import { emptySnapshot, emptyProvider, type Progress, type Project, type Settings } from '@/domain'

async function fixture(native = false) {
  const data = structuredClone(emptySnapshot)
  data.platform = 'windows'
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.checkUpdates = false
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
    repository:
      isWorktree || id === 'main-repo' ? repository : { id, name: id, path: `/repos/${id}` },
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
  let progress: ((event: Progress) => void) | undefined
  const refresh = vi.fn(async () => data)
  const refreshCaches = vi.fn(async () => data)
  const syncViewState = vi.fn(async () => {})
  const openAppLink = vi.fn(async () => {})
  const checkAppUpdate = vi.fn(async () => ({ version: '0.2.0', available: true }))
  const setDisabledShortcuts = vi.fn(async (disabledShortcuts: Settings['disabledShortcuts']) => {
    data.settings = { ...data.settings, disabledShortcuts }
    return { ...data }
  })
  const api: Backend = {
    ...backend,
    native,
    snapshot: async () => data,
    refresh,
    refreshCaches,
    syncViewState,
    openAppLink,
    checkAppUpdate,
    setDisabledShortcuts,
    saveSettings: async (settings) => {
      data.settings = settings
      return { ...data }
    },
    subscribe: async (handler) => {
      progress = handler
      return () => {
        progress = undefined
      }
    },
    subscribeMenu: async (handler) => {
      menu = handler
      return () => {
        menu = undefined
      }
    },
  }
  const mount = async () => {
    // Commit the snapshot first so the lazy page import starts before we await it.
    const app = await act(async () => render(<App api={api} />))
    await act(() => vi.dynamicImportSettled())
    return app
  }
  let app = await mount()
  return {
    api,
    refresh,
    refreshCaches,
    syncViewState,
    openAppLink,
    checkAppUpdate,
    setDisabledShortcuts,
    remount: async () => {
      app.unmount()
      app = await mount()
    },
    menu: (action: string) => act(() => menu?.(action)),
    progress: (stage: string, completed = 0) =>
      act(() =>
        progress?.({
          jobId: (refresh.mock.calls.at(-1) as unknown as [string])[0],
          stage,
          completed,
          total: null,
          message: '7 projects',
        }),
      ),
  }
}

it('opens cleanup preparation directly in the dialog without the page operation bar', async () => {
  const { api } = await fixture()
  let complete!: () => void
  api.prepare = () =>
    new Promise((resolve) => {
      complete = () =>
        resolve({
          id: 'review',
          kind: 'clean',
          createdAt: 0,
          items: [],
          warnings: [],
          useTrash: true,
        })
    })
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Project space' }))
  await screen.findByRole('heading', { name: 'Project space' })
  await user.click(screen.getByRole('checkbox', { name: 'Select main-repo' }))
  await user.click(screen.getByRole('button', { name: 'Review cleanup' }))
  const dialog = screen.getByRole('dialog', { name: 'Preparing review' })
  expect(within(dialog).getByRole('status').textContent).toContain('Clean project artifacts')
  expect(screen.queryByRole('button', { name: 'Cancel current task', hidden: true })).toBeNull()
  expect(document.querySelectorAll('[role="status"]')).toHaveLength(1)
  expect(screen.queryByRole('button', { name: 'Confirm operation' })).toBeNull()
  await act(async () => complete())
  expect(screen.getByRole('dialog', { name: 'Review operation' })).toBe(dialog)
})

it('describes the active scan phase independently of the current page', async () => {
  const { refresh, menu, progress } = await fixture(true)
  let finish!: () => void
  refresh.mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        finish = () => resolve(structuredClone(emptySnapshot))
      }),
  )
  await screen.findByRole('heading', { name: 'Overview' })
  menu('refresh')
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
  progress('discover', 42)
  expect(screen.getByRole('status').textContent).toContain('Scanning projects · 42 paths checked')
  menu('activity')
  await screen.findByRole('heading', { name: 'Activity' })
  expect(screen.getByRole('status').textContent).toContain('Scanning projects · 42 paths checked')
  progress('updates')
  expect(screen.getByRole('status').textContent).toContain('Checking tool updates')
  progress('measure-caches')
  expect(screen.getByRole('status').textContent).toContain('Measuring caches')
  await act(async () => finish())
  expect(screen.queryByRole('status')).toBeNull()
})

it('overview cards and suggested rows open the correct destinations with idle filters', async () => {
  await fixture()
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'Available updates' }))
  await screen.findByRole('heading', { name: 'Settings' })
  await user.click(screen.getByRole('button', { name: 'Overview' }))
  expect(screen.queryByRole('button', { name: 'Worktrees' })).toBeNull()
  await user.click(await screen.findByRole('button', { name: /2 inactive workspaces/ }))
  await screen.findByRole('heading', { name: 'Project space' })
  expect(screen.getByRole('switch', { name: 'Idle only' }).getAttribute('aria-checked')).toBe(
    'true',
  )
  expect(screen.queryByText('recent-repo')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Overview' }))
  await user.click(await screen.findByRole('button', { name: /Worktree artifacts/ }))
  await screen.findByRole('heading', { name: 'Project space' })
  const main = screen.getByRole('main')
  expect(within(main).getByTitle('/repos/main-repo')).toBeTruthy()
  expect(within(main).getByTitle('/elsewhere/feature')).toBeTruthy()
  expect(within(main).getByTitle('/repos/recent-repo')).toBeTruthy()
})

it('syncs native view commands with sidebar, theme, and bounded zoom', async () => {
  const { menu, syncViewState } = await fixture(true)
  await screen.findByRole('heading', { name: 'Overview' })
  menu('toggle-sidebar')
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({ sidebar: false, theme: 'system', zoom: 1 }),
  )
  menu('theme-dark')
  await waitFor(() => expect(document.documentElement.classList.contains('dark')).toBe(true))
  menu('zoom-in')
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({ sidebar: false, theme: 'dark', zoom: 1.1 }),
  )
  for (let i = 0; i < 10; i++) menu('zoom-in')
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({ sidebar: false, theme: 'dark', zoom: 1.5 }),
  )
  fireEvent.keyDown(window, { key: '0', ctrlKey: true })
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({ sidebar: false, theme: 'dark', zoom: 1 }),
  )
})

it('opens GitHub and issue links through the native browser integration and checks releases', async () => {
  const { menu, openAppLink, checkAppUpdate } = await fixture(true)
  const user = userEvent.setup()
  await screen.findByRole('heading', { name: 'Overview' })
  menu('about')
  await user.click(await screen.findByRole('link', { name: 'GitHub' }))
  expect(openAppLink).toHaveBeenCalledWith('github')
  await user.click(screen.getByRole('button', { name: 'Check for updates' }))
  await screen.findByText('New version available · 0.2.0')
  expect(checkAppUpdate).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('link', { name: 'View release page' }))
  expect(openAppLink).toHaveBeenCalledWith('latest')
  menu('issues')
  expect(openAppLink).toHaveBeenCalledWith('issues')
})

it('keeps missing ecosystems inspectable and removes capability labels from the catalog', async () => {
  await fixture()
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
  ).toMatch(/Install manager/)
  expect(screen.getByRole('button', { name: 'Install manager' }).hasAttribute('disabled')).toBe(
    false,
  )
})

it('routes native menu events to navigation, shortcuts, and rescan', async () => {
  const { menu, refresh } = await fixture()
  await screen.findByRole('heading', { name: 'Overview' })
  menu('projects')
  await screen.findByRole('heading', { name: 'Project space' })
  menu('shortcuts')
  expect(await screen.findByRole('dialog')).toBeTruthy()
  expect(screen.getByRole('heading', { name: 'Keyboard shortcuts' })).toBeTruthy()
  expect(screen.queryByText(/Getting started/)).toBeNull()
  expect(screen.queryByRole('switch', { name: 'Worktrees' })).toBeNull()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Close' }))
  menu('refresh')
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
  menu('unrecognized-command')
  expect(refresh).toHaveBeenCalledOnce()
})

it('handles Windows shortcuts delivered to the webview without repeating operations', async () => {
  const { refresh, refreshCaches } = await fixture(true)
  await screen.findByRole('heading', { name: 'Overview' })
  fireEvent.keyDown(window, { key: '4', ctrlKey: true })
  await screen.findByRole('heading', { name: 'Global caches' })
  await waitFor(() => expect(refreshCaches).toHaveBeenCalledWith(expect.any(String), false))
  refreshCaches.mockClear()
  fireEvent.keyDown(window, { key: 'F1' })
  expect(await screen.findByRole('dialog')).toBeTruthy()
  await userEvent.setup().click(screen.getByRole('button', { name: 'Close' }))
  fireEvent.keyDown(window, { key: 'r', ctrlKey: true, repeat: true })
  expect(refresh).not.toHaveBeenCalled()
  expect(refreshCaches).not.toHaveBeenCalled()
  fireEvent.keyDown(window, { key: 'r', ctrlKey: true })
  await waitFor(() => expect(refreshCaches).toHaveBeenCalledOnce())
  expect(refresh).not.toHaveBeenCalled()
})

it('disables shortcuts independently while leaving menu actions available', async () => {
  const { menu, refresh, syncViewState, setDisabledShortcuts } = await fixture(true)
  const user = userEvent.setup()
  await screen.findByRole('heading', { name: 'Overview' })
  menu('shortcuts')
  for (const name of [
    'Rescan',
    'Project space',
    'Toggle sidebar',
    'Zoom in',
    'Keyboard shortcuts',
  ]) {
    const control = screen.getByRole('switch', { name })
    await user.click(control)
    await waitFor(() => expect(control.getAttribute('aria-checked')).toBe('false'))
  }
  expect(setDisabledShortcuts).toHaveBeenLastCalledWith([
    'refresh',
    'projects',
    'toggle-sidebar',
    'zoom-in',
    'shortcuts',
  ])
  await user.click(screen.getByRole('button', { name: 'Close' }))
  syncViewState.mockClear()
  for (const key of ['r', '3', 'b', '+']) {
    expect(fireEvent.keyDown(window, { key, ctrlKey: true })).toBe(false)
  }
  fireEvent.keyDown(window, { key: 'F1' })
  expect(refresh).not.toHaveBeenCalled()
  expect(syncViewState).not.toHaveBeenCalled()
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(screen.getByRole('heading', { name: 'Overview' })).toBeTruthy()

  // Disabling one navigation shortcut must not disable the remaining pages.
  fireEvent.keyDown(window, { key: '4', ctrlKey: true })
  await screen.findByRole('heading', { name: 'Global caches' })
  menu('projects')
  await screen.findByRole('heading', { name: 'Project space' })
  menu('toggle-sidebar')
  menu('zoom-in')
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({
      sidebar: false,
      theme: 'system',
      zoom: 1.1,
    }),
  )
  menu('refresh')
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
  menu('shortcuts')
  await screen.findByRole('dialog', { name: 'Keyboard shortcuts' })
})

it('loads saved shortcut switches after reopening the app and restores their defaults', async () => {
  const { menu, remount, setDisabledShortcuts, syncViewState } = await fixture(true)
  const user = userEvent.setup()
  await screen.findByRole('heading', { name: 'Overview' })
  menu('shortcuts')
  await user.click(screen.getByRole('switch', { name: 'Toggle sidebar' }))
  await waitFor(() => expect(setDisabledShortcuts).toHaveBeenCalledWith(['toggle-sidebar']))
  await remount()
  await screen.findByRole('heading', { name: 'Overview' })
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({
      sidebar: true,
      theme: 'system',
      zoom: 1,
    }),
  )
  syncViewState.mockClear()
  fireEvent.keyDown(window, { key: 'b', ctrlKey: true })
  expect(syncViewState).not.toHaveBeenCalled()
  fireEvent.keyDown(window, { key: 'F1' })
  await screen.findByRole('dialog', { name: 'Keyboard shortcuts' })
  expect(screen.getByRole('switch', { name: 'Toggle sidebar' }).getAttribute('aria-checked')).toBe(
    'false',
  )
  await user.click(screen.getByRole('button', { name: 'Restore default shortcuts' }))
  await waitFor(() =>
    expect(
      screen.getByRole('switch', { name: 'Toggle sidebar' }).getAttribute('aria-checked'),
    ).toBe('true'),
  )
  expect(setDisabledShortcuts).toHaveBeenLastCalledWith([])
  await user.click(screen.getByRole('button', { name: 'Close' }))
  fireEvent.keyDown(window, { key: 'b', ctrlKey: true })
  await waitFor(() =>
    expect(syncViewState).toHaveBeenLastCalledWith({
      sidebar: false,
      theme: 'system',
      zoom: 1,
    }),
  )
})

it('keeps the shortcut enabled and reports an unsuccessful save', async () => {
  const { menu, setDisabledShortcuts, refresh } = await fixture(true)
  const user = userEvent.setup()
  setDisabledShortcuts.mockRejectedValueOnce(new Error('Unable to save preferences'))
  await screen.findByRole('heading', { name: 'Overview' })
  menu('shortcuts')
  await user.click(screen.getByRole('switch', { name: 'Rescan' }))
  await waitFor(() => expect(setDisabledShortcuts).toHaveBeenCalledOnce())
  expect(screen.getByRole('switch', { name: 'Rescan' }).getAttribute('aria-checked')).toBe('true')
  await user.click(screen.getByRole('button', { name: 'Close' }))
  expect((await screen.findByRole('alert')).textContent).toContain('Unable to save preferences')
  fireEvent.keyDown(window, { key: 'r', ctrlKey: true })
  await waitFor(() => expect(refresh).toHaveBeenCalledOnce())
})

it('reports an update check failure and allows retrying without inventing a newer version', async () => {
  const { menu, checkAppUpdate } = await fixture(true)
  checkAppUpdate.mockRejectedValueOnce(new Error('Network unavailable'))
  checkAppUpdate.mockResolvedValueOnce({ version: '0.1.0', available: false })
  await screen.findByRole('heading', { name: 'Overview' })
  menu('check-update')
  await screen.findByText('Unable to check for updates. Retry or visit the release page.')
  await userEvent.setup().click(screen.getByRole('button', { name: 'Retry' }))
  await screen.findByText('You are up to date.')
  expect(checkAppUpdate).toHaveBeenCalledTimes(2)
})
