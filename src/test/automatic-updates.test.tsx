import { Fragment, StrictMode } from 'react'
import { act, render, waitFor } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { backend } from '@/bridge'
import { emptyProvider, emptySnapshot, type Provider, type ProviderId } from '@/domain'
import { StoreProvider, useStore } from '@/store'

async function fixture(id: ProviderId = 'go', enabled = true, strict = false) {
  let data = structuredClone(emptySnapshot)
  data.settings.scanOnLaunch = false
  data.settings.checkUpdates = enabled
  const provider = emptyProvider(id)
  provider.packageManagers = [
    {
      id: 'manager',
      name: 'mise',
      version: '1.0.0',
      latest: null,
      updateStatus: 'unknown',
      source: 'mise',
      runtime: null,
      path: '/tools/mise',
      size: null,
      canUpdate: true,
      canRemove: true,
      note: null,
    },
  ]
  provider.tools = [{ ...provider.packageManagers[0], id: 'global', name: 'global' }]
  data.inventory.providers = [provider]
  let resolve!: (provider: Provider) => void
  let reject!: (cause: Error) => void
  const check = vi.fn(
    () =>
      new Promise<Provider>((yes, no) => {
        resolve = yes
        reject = no
      }),
  )
  const refresh = vi.fn(async () => data)
  const cancel = vi.fn(async () => {})
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'operation',
    createdAt: 0,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  let state!: ReturnType<typeof useStore>
  function Probe() {
    state = useStore()
    return null
  }
  const Wrapper = strict ? StrictMode : Fragment
  const mounted = render(
    <Wrapper>
      <StoreProvider
        api={{
          ...backend,
          native: true,
          snapshot: async () => structuredClone(data),
          refresh,
          checkToolUpdates: check,
          cancel,
          prepare,
          saveSettings: async (settings) => ({ ...data, settings }),
        }}
      >
        <Probe />
      </StoreProvider>
    </Wrapper>,
  )
  await waitFor(() => expect(state.loaded).toBe(true))
  act(() => state.go('env', id))
  return {
    ...mounted,
    provider,
    check,
    refresh,
    cancel,
    prepare,
    state: () => state,
    resolve: (p: Provider) => act(async () => resolve(p)),
    reject: () => act(async () => reject(new Error('offline'))),
    change: async (p: Provider) => {
      data = { ...data, inventory: { ...data.inventory, providers: [p] } }
      await act(async () => state.reload())
    },
  }
}
afterEach(() => vi.restoreAllMocks())

it.each(['js', 'py', 'go', 'jvm', 'rust', 'ollama'] as ProviderId[])(
  'automatically checks %s without scanning or blocking operations',
  async (id) => {
    const f = await fixture(id)
    await waitFor(() => expect(f.check).toHaveBeenCalledTimes(1))
    expect(f.state().busy).toBe(false)
    expect(f.state().task).toBeNull()
    await act(async () => f.state().prepare({ kind: 'cleanCaches', ids: ['cache'] }))
    expect(f.prepare).toHaveBeenCalledOnce()
    const updated = structuredClone(f.provider)
    for (const tool of [...updated.packageManagers, ...updated.tools]) {
      tool.latest = '2.0.0'
      tool.updateStatus = 'major'
    }
    await f.resolve(updated)
    expect(f.state().data.inventory.providers[0].tools[0].latest).toBe('2.0.0')
    act(() => {
      f.state().go('overview')
      f.state().go('env', id)
    })
    expect(f.check).toHaveBeenCalledTimes(1)
    expect(f.refresh).not.toHaveBeenCalled()
  },
)

it('respects the network setting and discards an in-flight result when disabled', async () => {
  const f = await fixture('go', false)
  expect(f.check).not.toHaveBeenCalled()
  await act(async () => {
    await f.state().saveSettings({ ...f.state().data.settings, checkUpdates: true })
  })
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(1))
  await act(async () => {
    await f.state().saveSettings({ ...f.state().data.settings, checkUpdates: false })
  })
  const updated = structuredClone(f.provider)
  updated.packageManagers[0].latest = '2.0.0'
  await f.resolve(updated)
  expect(f.state().data.inventory.providers[0].packageManagers[0].latest).toBeNull()
  expect(f.cancel).toHaveBeenCalledOnce()
})

it('deduplicates checks, retries manually after failure, and refreshes expired results', async () => {
  const clock = vi.spyOn(Date, 'now').mockReturnValue(1000000)
  const f = await fixture()
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(1))
  await act(async () => f.state().checkToolUpdates('go'))
  expect(f.check).toHaveBeenCalledTimes(1)
  await f.reject()
  act(() => {
    f.state().go('overview')
    f.state().go('env', 'go')
  })
  expect(f.check).toHaveBeenCalledTimes(1)
  act(() => {
    void f.state().checkToolUpdates('go')
  })
  expect(f.check).toHaveBeenCalledTimes(2)
  const updated = structuredClone(f.provider)
  for (const tool of [...updated.packageManagers, ...updated.tools]) {
    tool.latest = tool.version
    tool.updateStatus = 'latest'
  }
  await f.resolve(updated)
  clock.mockReturnValue(2000000)
  act(() => window.dispatchEvent(new Event('focus')))
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(3))
})

it('does not restore removed tools or overwrite new versions when an old check finishes', async () => {
  const f = await fixture()
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(1))
  const changed = structuredClone(f.provider)
  changed.tools = []
  changed.packageManagers[0].version = '3.0.0'
  await f.change(changed)
  const stale = structuredClone(f.provider)
  stale.packageManagers[0].latest = '2.0.0'
  await f.resolve(stale)
  const current = f.state().data.inventory.providers[0]
  expect(current.tools).toEqual([])
  expect(current.packageManagers[0].version).toBe('3.0.0')
  expect(current.packageManagers[0].latest).toBeNull()
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(2))
})

it('keeps automatic lookup working under Strict Mode and cancels on unmount', async () => {
  const f = await fixture('go', true, true)
  await waitFor(() => expect(f.check).toHaveBeenCalledTimes(1))
  f.unmount()
  expect(f.cancel).toHaveBeenCalledOnce()
  await f.resolve(f.provider)
})
