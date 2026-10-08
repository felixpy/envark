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
  await user.hover(within(screen.getByText('external').closest('tr')!).getByText('Owner-managed'))
  expect((await screen.findByRole('tooltip')).textContent).toBe('Cache owner is unavailable.')
  await user.click(screen.getByRole('button', { name: 'Review cleanup (2)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith({ kind: 'cleanCaches', ids: ['npm', 'pnpm'] }),
  )
})
