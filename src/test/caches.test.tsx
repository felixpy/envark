import { act } from '@testing-library/react'
import { expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { backend } from '@/bridge'
import { emptySnapshot } from '@/domain'
import { StoreProvider, useStore } from '@/store'
import Caches from '@/views/Caches'
import { TooltipProvider } from '@/components/ui/tooltip'

it('selects every eligible cache, shows a mixed state, and explains read-only rows', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.inventory.caches = ['npm', 'pnpm', 'external'].map((id) => ({
    id,
    provider: 'js',
    name: id,
    path: `/cache/${id}`,
    size: { bytes: 42, files: 1, skipped: 0, complete: true },
    strategy: 'native',
    warning: id === 'external' ? 'Cache owner is unavailable.' : '',
    canClean: id !== 'external',
  }))
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'clean',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  render(
    <StoreProvider api={{ ...backend, snapshot: async () => data, prepare }}>
      <Caches />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  const user = userEvent.setup()
  await user.click(await screen.findByRole('checkbox', { name: 'Select npm' }))
  expect(
    screen
      .getByRole('checkbox', { name: 'Select all eligible caches' })
      .getAttribute('aria-checked'),
  ).toBe('mixed')
  await user.click(screen.getByRole('checkbox', { name: 'Select all eligible caches' }))
  expect(screen.queryByRole('checkbox', { name: 'Select external' })).toBeNull()
  expect(screen.getAllByRole('table')).toHaveLength(2)
  const external = screen.getByText('external').closest('section')!
  expect(within(external).queryByRole('checkbox')).toBeNull()
  expect(screen.queryByText('Cache owner is unavailable.')).toBeNull()
  await user.hover(
    within(screen.getByText('external').closest('tr')!).getByText('Cleanup unavailable'),
  )
  expect((await screen.findByRole('tooltip')).textContent).toBe('Cache owner is unavailable.')
  await user.click(screen.getByRole('button', { name: 'Review cleanup (2)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith(
      { kind: 'cleanCaches', ids: ['npm', 'pnpm'] },
      expect.any(String),
    ),
  )
})

it('offers pip, Cargo and Gradle cleanup in the app with accurate strategies', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.useTrash = true
  data.inventory.caches = [
    { id: 'pip', name: 'pip', strategy: 'pip-purge', provider: 'py' as const },
    { id: 'cargo', name: 'Cargo registry', strategy: 'cargo-registry', provider: 'rust' as const },
    { id: 'gradle', name: 'Gradle caches', strategy: 'gradle-caches', provider: 'jvm' as const },
    {
      id: 'dists',
      name: 'Gradle distributions',
      strategy: 'gradle-dists',
      provider: 'jvm' as const,
    },
  ].map((cache) => ({
    ...cache,
    path: `/cache/${cache.id}`,
    size: { bytes: 42, files: 1, skipped: 0, complete: true },
    canClean: true,
    warning: '',
  }))
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'clean',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  const refresh = vi.fn()
  render(
    <StoreProvider api={{ ...backend, snapshot: async () => data, prepare, refresh }}>
      <Caches />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  const user = userEvent.setup()
  await user.click(await screen.findByRole('checkbox', { name: 'Select all eligible caches' }))
  expect(screen.getByText('Move to Trash')).toBeTruthy()
  expect(screen.getAllByText('Prune expired caches')).toHaveLength(2)
  expect(screen.queryByText('Owner-managed')).toBeNull()
  await user.click(screen.getByRole('button', { name: 'Review cleanup (4)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith(
      { kind: 'cleanCaches', ids: ['pip', 'cargo', 'gradle', 'dists'] },
      expect.any(String),
    ),
  )
  expect(refresh).not.toHaveBeenCalled()
})

it('refreshes caches without scanning project roots and keeps review available afterward', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  const refresh = vi.fn(async () => data)
  const refreshCaches = vi.fn(async () => data)
  let state!: ReturnType<typeof useStore>
  function Probe() {
    state = useStore()
    return <Caches />
  }
  render(
    <StoreProvider
      api={{ ...backend, snapshot: async () => data, refresh, refreshCaches } as typeof backend}
    >
      <Probe />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  await waitFor(() => expect(state.loaded).toBe(true))
  act(() => state.go('caches'))
  await userEvent.setup().click(screen.getByRole('button', { name: 'Refresh caches' }))
  await waitFor(() => expect(state.busy).toBe(false))
  expect(refreshCaches).toHaveBeenCalledOnce()
  expect(refresh).not.toHaveBeenCalled()
  expect(state.busy).toBe(false)
})

it('checks cleanup prerequisites automatically, explains blocked caches and rechecks without measuring', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.checkUpdates = false
  data.inventory.caches = [
    { id: 'npm', name: 'npm', provider: 'js' as const, strategy: 'npm-verify', canClean: true },
    {
      id: 'gradle',
      name: 'Gradle caches',
      provider: 'jvm' as const,
      strategy: 'gradle-caches',
      canClean: true,
    },
    {
      id: 'cargo',
      name: 'Cargo registry',
      provider: 'rust' as const,
      strategy: 'cargo-registry',
      canClean: true,
    },
  ].map((cache) => ({
    ...cache,
    path: `/cache/${cache.id}`,
    size: { bytes: 42, files: 1, skipped: 0, complete: true },
    warning: '',
  }))
  const checked = structuredClone(data)
  checked.inventory.caches[1].canClean = false
  checked.inventory.caches[1].cleanupIssue = {
    reason: 'toolUnavailable',
    detail: 'No installed Gradle distribution is available.',
  }
  checked.inventory.caches[2].canClean = false
  checked.inventory.caches[2].cleanupIssue = {
    reason: 'busy',
    detail: 'Cargo is using this cache.',
  }
  const refresh = vi.fn(async () => data)
  const refreshCaches = vi.fn(async (_jobId: string, _measure?: boolean) =>
    structuredClone(checked),
  )
  const prepare = vi.fn(async () => ({
    id: 'clean',
    kind: 'clean',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: false,
  }))
  let state!: ReturnType<typeof useStore>
  function Probe() {
    state = useStore()
    return <Caches />
  }
  render(
    <StoreProvider
      api={{
        ...backend,
        native: true,
        snapshot: async () => data,
        refresh,
        refreshCaches,
        prepare,
      }}
    >
      <Probe />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  const user = userEvent.setup()
  await screen.findByText(
    'No usable cleanup tool was found. Install or repair it in Environments & tools.',
  )
  expect(refreshCaches).toHaveBeenCalledWith(expect.any(String), false)
  expect(screen.queryByRole('checkbox', { name: 'Select Gradle caches' })).toBeNull()
  expect(screen.queryByRole('checkbox', { name: 'Select Cargo registry' })).toBeNull()
  const cargo = screen.getByText('Cargo registry').closest('tr')!
  expect(
    within(cargo).getByText(
      'A build or download is using this cache. Check again after it finishes.',
    ),
  ).toBeTruthy()
  checked.inventory.caches[2].canClean = true
  checked.inventory.caches[2].cleanupIssue = null
  await user.click(within(cargo).getByRole('button', { name: 'Check again' }))
  await screen.findByRole('checkbox', { name: 'Select Cargo registry' })
  expect(refreshCaches).toHaveBeenCalledTimes(2)
  expect(refreshCaches.mock.calls.every((call) => call[1] === false)).toBe(true)
  expect(refresh).not.toHaveBeenCalled()
  await user.click(
    within(screen.getByText('Gradle caches').closest('tr')!).getByRole('button', {
      name: 'Manage cleanup tool',
    }),
  )
  expect(state.provider).toBe('jvm')
  expect(state.focus.tab).toBe('pm')
  await user.click(screen.getByRole('checkbox', { name: 'Select all eligible caches' }))
  await user.click(screen.getByRole('button', { name: 'Review cleanup (2)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith(
      { kind: 'cleanCaches', ids: ['npm', 'cargo'] },
      expect.any(String),
    ),
  )
})
