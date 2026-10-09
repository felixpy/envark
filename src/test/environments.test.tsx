import { expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { backend, type Backend } from '@/bridge'
import { emptyProvider, emptySnapshot, type Provider, type ProviderId, type Tool } from '@/domain'
import { StoreProvider } from '@/store'
import { TaskProgress } from '@/components/TaskProgress'
import Environments from '@/views/Environments'
import { TooltipProvider } from '@/components/ui/tooltip'

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
    { wrapper: TooltipProvider },
  )
  return prepare
}

it('bulk tool updates include only tools with a verified update capability', async () => {
  const provider = emptyProvider('js')
  provider.tools = ['owned', 'external', 'ahead', 'unknown'].map((id): Tool => ({
    id,
    name: id,
    version: '1.0.0',
    latest: '2.0.0',
    updateStatus: id === 'ahead' ? 'ahead' : id === 'unknown' ? 'unknown' : 'major',
    source: 'npm',
    runtime: null,
    path: null,
    size: null,
    canUpdate: id !== 'external',
    canRemove: false,
    note: null,
  }))
  const prepare = fixture(provider)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Global tools' }))
  await user.click(screen.getByRole('checkbox', { name: 'Select all updatable tools' }))
  await user.click(screen.getByRole('button', { name: 'Update selected (1)' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith(
      { kind: 'updateTools', provider: 'js', ids: ['owned'] },
      expect.any(String),
    ),
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
    expect(prepare).toHaveBeenCalledWith(
      {
        kind: 'removeAssets',
        provider: 'playwright',
        ids: ['complete'],
      },
      expect.any(String),
    ),
  )
})

it('does not mistake an inherited active runtime for the manager default', async () => {
  const provider = emptyProvider('js')
  provider.managers = [
    {
      name: 'fnm',
      version: 'test',
      path: '/example/fnm',
      supportsInstall: true,
      supportsDefault: true,
    },
  ]
  provider.runtimes = [
    {
      id: 'runtime',
      selector: null,
      activeKnown: true,
      version: '24.0.0',
      manager: 'fnm',
      path: '/example/node',
      active: true,
      managed: true,
      size: null,
      note: null,
    },
  ]
  const prepare = fixture(provider)
  const user = userEvent.setup()
  await screen.findByText('Current environment')
  await user.click(screen.getByRole('button', { name: 'Set default' }))
  await waitFor(() =>
    expect(prepare).toHaveBeenCalledWith(
      {
        kind: 'setDefault',
        provider: 'js',
        id: 'runtime',
      },
      expect.any(String),
    ),
  )
  expect(screen.getByRole('button', { name: 'Remove' }).getAttribute('aria-disabled')).toBe('true')
})

for (const id of ['js', 'py', 'jvm', 'rust', 'go'] as ProviderId[]) {
  for (const tab of ['Global tools', 'Package managers']) {
    it(`${id} ${tab} checks versions without refreshing projects or environments`, async () => {
      const provider = emptyProvider(id)
      const data = structuredClone(emptySnapshot)
      data.settings.language = 'en'
      data.settings.scanOnLaunch = false
      data.settings.checkUpdates = true
      data.inventory.providers = [provider]
      const refresh = vi.fn(async () => data)
      let complete!: (provider: Provider) => void
      const checkToolUpdates = vi.fn(
        () =>
          new Promise<Provider>((resolve) => {
            complete = resolve
          }),
      )
      const cancel = vi.fn(async () => {})
      const api: Backend = {
        ...backend,
        native: false,
        snapshot: async () => data,
        refresh,
        checkToolUpdates,
        cancel,
      }
      render(
        <StoreProvider api={api}>
          <TaskProgress />
          <Environments id={id} />
        </StoreProvider>,
        { wrapper: TooltipProvider },
      )
      const user = userEvent.setup()
      await user.click(await screen.findByRole('tab', { name: tab }))
      await user.click(screen.getByRole('button', { name: 'Check updates again' }))
      expect(checkToolUpdates).toHaveBeenCalledWith(id, expect.any(String))
      expect(screen.getByRole('status').textContent).toContain('Checking tool updates')
      expect(screen.queryByText(/Scanning/)).toBeNull()
      await user.click(screen.getByRole('button', { name: 'Check updates again' }))
      expect(checkToolUpdates).toHaveBeenCalledTimes(1)
      complete(provider)
      await waitFor(() => expect(screen.queryByRole('status')).toBeNull())
      expect(refresh).not.toHaveBeenCalled()
    })
  }
}

it('a discovered Bun update can be reviewed from the package manager tab', async () => {
  const provider = emptyProvider('js')
  provider.packageManagers = [
    {
      id: 'bun',
      name: 'bun',
      version: '1.3.0',
      latest: '1.4.0',
      updateStatus: 'minor',
      source: 'bun',
      runtime: null,
      path: '/example/.bun/bin/bun',
      size: null,
      canUpdate: true,
      canRemove: false,
      note: null,
    },
  ]
  const prepare = fixture(provider)
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Package managers' }))
  await user.click(screen.getByRole('button', { name: 'Update' }))
  expect(prepare).toHaveBeenCalledWith(
    { kind: 'updateTool', provider: 'js', id: 'bun' },
    expect.any(String),
  )
})

it.each(['fnm', 'nvm'])(
  'updates %s itself from the runtime tab without removing Node versions',
  async (name) => {
    const provider = emptyProvider('js')
    provider.managers = [
      {
        name,
        version: '1.0.0',
        path: `/managers/${name}`,
        supportsInstall: true,
        supportsDefault: true,
      },
    ]
    provider.packageManagers = [
      {
        id: name,
        name,
        version: '1.0.0',
        latest: '2.0.0',
        updateStatus: 'major',
        source: `${name}-script`,
        runtime: null,
        path: `/managers/${name}`,
        size: null,
        canUpdate: true,
        canRemove: false,
        note: null,
      },
    ]
    const prepare = fixture(provider)
    const user = userEvent.setup()
    await screen.findByText('Version managers')
    expect(screen.getByText('Official installer')).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Update' }))
    await waitFor(() =>
      expect(prepare).toHaveBeenCalledWith(
        { kind: 'updateTool', provider: 'js', id: name },
        expect.any(String),
      ),
    )
  },
)
