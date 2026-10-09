import { expect, it, vi } from 'vitest'
import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { backend } from '@/bridge'
import { emptyProvider, emptySnapshot, metadata, type Provider, type Tool } from '@/domain'
import { StoreProvider, useStore } from '@/store'
import { TooltipProvider } from '@/components/ui/tooltip'
import Catalog, { updateTab } from '@/views/Catalog'
import Environments from '@/views/Environments'
import Overview from '@/views/Overview'

function outdated(name: string): Tool {
  return {
    id: `tool-${name}`,
    name,
    version: '1.0.0',
    latest: '2.0.0',
    updateStatus: 'major',
    source: 'fixture',
    runtime: null,
    path: `/fixture/${name}`,
    size: null,
    canUpdate: true,
    canRemove: true,
    note: null,
  }
}
function Route() {
  const s = useStore()
  if (!s.loaded) return null
  if (s.provider) return <Environments key={s.navigationKey} id={s.provider} />
  if (s.focus.filter === 'updates') return <Catalog />
  return <button onClick={() => s.go('env', null, { filter: 'updates' })}>Available updates</button>
}
function fixture(provider: Provider) {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.inventory.providers = [provider]
  const prepare = vi.fn(async () => ({
    id: 'review',
    kind: 'updateTool',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  render(
    <StoreProvider api={{ ...backend, native: false, snapshot: async () => data, prepare }}>
      <Route />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  return prepare
}

for (const [id, name, target] of [
  ['ollama', 'ollama', 'runtime'],
  ['rust', 'rustup', 'runtime'],
  ['go', 'mise', 'runtime'],
  ['js', 'fnm', 'runtime'],
  ['py', 'pip', 'pm'],
  ['js', 'typescript', 'global'],
] as const) {
  it(`opens the ${target} table containing the ${name} update from the catalog`, async () => {
    const provider = emptyProvider(id)
    const tool = outdated(name)
    if (target === 'global' || id === 'ollama') provider.tools = [tool]
    else provider.packageManagers = [tool]
    if (target === 'runtime' && id !== 'ollama') {
      provider.managers = [
        {
          name,
          version: '1.0.0',
          path: `/fixture/${name}`,
          supportsInstall: true,
          supportsDefault: true,
        },
      ]
    }
    const prepare = fixture(provider)
    const user = userEvent.setup()
    await user.click(await screen.findByRole('button', { name: 'Available updates' }))
    const card = screen.getByRole('button', { name: new RegExp(metadata[id].name) })
    if (id === 'rust') {
      card.focus()
      await user.keyboard('{Enter}')
    } else {
      await user.click(card)
    }
    const label =
      id === 'ollama'
        ? 'Application & service'
        : target === 'runtime'
          ? 'Runtimes'
          : target === 'pm'
            ? 'Package managers'
            : 'Global tools'
    expect(screen.getByRole('tab', { name: label, selected: true })).toBeTruthy()
    await user.click(screen.getByRole('button', { name: 'Update' }))
    await waitFor(() =>
      expect(prepare).toHaveBeenCalledWith(
        { kind: 'updateTool', provider: id, id: tool.id },
        expect.any(String),
      ),
    )
  })
}

it('does not route a package-manager update to an unrelated up-to-date version manager', () => {
  const provider = emptyProvider('js')
  provider.managers = [
    {
      name: 'fnm',
      version: '1.0.0',
      path: '/fixture/fnm',
      supportsInstall: true,
      supportsDefault: true,
    },
  ]
  provider.packageManagers = [
    { ...outdated('fnm'), version: '2.0.0', updateStatus: 'latest' },
    outdated('npm'),
  ]
  expect(updateTab(provider)).toBe('pm')
})

it('keeps global updates reachable when the same environment also has manager updates', () => {
  const provider = emptyProvider('js')
  provider.tools = [outdated('typescript')]
  provider.packageManagers = [outdated('fnm')]
  provider.managers = [
    {
      name: 'fnm',
      version: '1.0.0',
      path: '/fixture/fnm',
      supportsInstall: true,
      supportsDefault: true,
    },
  ]
  expect(updateTab(provider)).toBe('global')
})

it('counts a shared mise update once while keeping separate installations distinct', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  const go = emptyProvider('go')
  const java = emptyProvider('jvm')
  go.packageManagers = [outdated('mise')]
  java.packageManagers = [
    { ...outdated('mise') },
    { ...outdated('mise'), id: 'other-mise-installation', path: '/other/mise' },
  ]
  data.inventory.providers = [go, java]
  render(
    <StoreProvider api={{ ...backend, native: false, snapshot: async () => data }}>
      <Overview />
    </StoreProvider>,
    { wrapper: TooltipProvider },
  )
  await waitFor(() =>
    expect(
      within(screen.getByRole('button', { name: 'Available updates' })).getByText('2'),
    ).toBeTruthy(),
  )
})
