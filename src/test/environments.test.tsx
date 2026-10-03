import { expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { backend } from '@/bridge'
import { emptyProvider, emptySnapshot, type Provider, type Tool } from '@/domain'
import { StoreProvider } from '@/store'
import Environments from '@/views/Environments'

function fixture(provider: Provider) {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.inventory.providers = [provider]
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'operation',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  render(
    <StoreProvider api={{ ...backend, native: false, snapshot: async () => data, prepare }}>
      <Environments id={provider.id} />
    </StoreProvider>,
  )
  return prepare
}

it('bulk tool updates include only tools with a verified update capability', async () => {
  const provider = emptyProvider('js')
  provider.tools = ['owned', 'external'].map((id): Tool => ({
    id,
    name: id,
    version: '1.0.0',
    latest: '2.0.0',
    source: 'npm',
    runtime: null,
    path: null,
    size: null,
    canUpdate: id === 'owned',
    canRemove: false,
    note: null,
  }))
  const prepare = fixture(provider)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Global tools' }))
  await user.click(screen.getByRole('checkbox', { name: 'Select all updatable tools' }))
  await user.click(screen.getByRole('button', { name: 'Update selected (1)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith({ kind: 'updateTools', provider: 'js', ids: ['owned'] }),
  )
})

it('bulk resource removal excludes resources with incomplete size scans', async () => {
  const provider = emptyProvider('playwright')
  provider.assets = ['complete', 'partial'].map((id) => ({
    id,
    name: id,
    version: '1',
    path: `/browsers/${id}`,
    size: { bytes: 1024, files: 1, skipped: 0, complete: id === 'complete' },
    lastUsed: null,
    modified: null,
    usedBy: [],
    canRemove: true,
    note: null,
  }))
  const prepare = fixture(provider)
  const user = userEvent.setup()
  await screen.findByRole('checkbox', { name: 'Select complete' })
  await user.click(screen.getByRole('checkbox', { name: 'Select all resources' }))
  await user.click(screen.getByRole('button', { name: 'Review removal' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith({
      kind: 'removeAssets',
      provider: 'playwright',
      ids: ['complete'],
    }),
  )
})
