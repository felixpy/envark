import { expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { backend } from '@/bridge'
import { emptySnapshot } from '@/domain'
import { StoreProvider } from '@/store'
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
