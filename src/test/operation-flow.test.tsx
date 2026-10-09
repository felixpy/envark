import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import { backend, type Backend } from '@/bridge'
import { emptySnapshot, type Plan, type Progress } from '@/domain'
import { StoreProvider, useStore } from '@/store'
import { OperationDialog } from '@/components/OperationDialog'
import { TaskProgress } from '@/components/TaskProgress'

const plan: Plan = {
  id: 'plan',
  kind: 'operation',
  createdAt: 0,
  useTrash: true,
  warnings: [],
  items: [{ title: 'Install runtime', path: null, command: null, restore: null, bytes: 0 }],
}

async function fixture(overrides: Partial<Backend> = {}) {
  const data = structuredClone(emptySnapshot)
  data.settings.scanOnLaunch = false
  data.settings.language = 'en'
  let state!: ReturnType<typeof useStore>
  let emit!: (event: Progress) => void
  const api = {
    ...backend,
    native: false,
    snapshot: vi.fn(async () => data),
    refresh: vi.fn(async () => data),
    prepare: vi.fn(async () => plan),
    cancel: vi.fn(async () => {}),
    subscribe: async (callback: typeof emit) => {
      emit = callback
      return () => {}
    },
    ...overrides,
  }
  function Probe() {
    state = useStore()
    return (
      <>
        <span data-testid="loaded">{String(state.loaded)}</span>
        {!state.plan && <TaskProgress />}
        <OperationDialog />
      </>
    )
  }
  const mounted = render(
    <StoreProvider api={api}>
      <Probe />
    </StoreProvider>,
  )
  await waitFor(() => expect(screen.getByTestId('loaded').textContent).toBe('true'))
  return { api, emit: (event: Progress) => act(() => emit(event)), state: () => state, ...mounted }
}

afterEach(() => vi.useRealTimers())

it('makes preparation visible, prevents competing tasks, filters stale progress, and cancels the correct job', async () => {
  let reject!: (error: Error) => void
  const prepare = vi.fn(
    () =>
      new Promise<Plan>((_, fail) => {
        reject = fail
      }),
  )
  const f = await fixture({ prepare })
  let pending!: Promise<void>
  act(() => {
    pending = f.state().prepare({ kind: 'cleanCaches', ids: ['npm'] })
  })
  expect(screen.getByRole('status').textContent).toContain('Checking selected items before review')
  expect(screen.getByRole('status').textContent).toContain('Clean shared caches')
  const jobId = (prepare.mock.calls[0] as unknown as [unknown, string])[1]
  await act(async () => {
    await f.state().prepare({ kind: 'cleanCaches', ids: ['uv'] })
    await f.state().refresh()
  })
  expect(prepare).toHaveBeenCalledTimes(1)
  expect(f.api.refresh).not.toHaveBeenCalled()
  f.emit({ jobId: 'stale-scan', stage: 'discover', completed: 999, total: null, message: '' })
  expect(screen.getByRole('status').textContent).not.toContain('999')
  f.emit({ jobId, stage: 'prepare', completed: 1, total: 2, message: '/projects/selected' })
  expect(screen.getByRole('status').textContent).toContain('/projects/selected')
  expect(screen.getByRole('status').textContent).toContain('1/2')
  await act(() => f.state().cancel())
  expect(f.api.cancel).toHaveBeenCalledWith(jobId)
  await act(async () => {
    reject(new Error('The operation was cancelled.'))
    await pending
  })
  expect(f.state().busy).toBe(false)
  expect(f.state().error).toBeNull()
  expect(screen.queryByRole('status')).toBeNull()
})

it('summarizes large results, exposes failures, and keeps Done outside the scrolling details', async () => {
  const f = await fixture({
    prepare: async () => ({ ...plan, kind: 'clean' }),
    execute: async () => ({
      items: [
        ...Array.from({ length: 120 }, (_, i) => ({
          title: `Clean ${i}`,
          status: 'success',
          message: 'Moved to Trash.',
          removedBytes: 1024,
        })),
        { title: 'Retained source', status: 'skipped', message: 'Kept.', removedBytes: 0 },
        { title: 'Permission error', status: 'failed', message: 'Access denied.', removedBytes: 0 },
        {
          title: 'Interrupted item',
          status: 'cancelled',
          message: 'May be partially complete.',
          removedBytes: 0,
        },
      ],
      removedBytes: 122880,
      reclaimedBytes: null,
      cancelled: true,
    }),
  })
  await act(() => f.state().prepare({ kind: 'cleanCaches', ids: ['cache'] }))
  await act(() => f.state().execute())
  expect(screen.getByText('Completed 120 · Kept 1 · Failed 1 · Interrupted 1')).toBeTruthy()
  expect(
    within(screen.getByRole('region', { name: 'Failed items' })).getByText('Access denied.'),
  ).toBeTruthy()
  expect(screen.getByText('Interrupted item')).toBeTruthy()
  const details = screen.getByText('View completed items (120)').closest('details')!
  expect(details.open).toBe(false)
  expect(screen.getByText('View kept items (1)').closest('details')!.open).toBe(false)
  const done = screen.getByRole('button', { name: 'Done' })
  expect(done.closest('[data-slot="operation-scroll"]')).toBeNull()
  expect(done.closest('[data-slot="dialog-footer"]')).toBeTruthy()
  fireEvent.click(screen.getByText('View completed items (120)'))
  expect(details.open).toBe(true)
  fireEvent.click(done)
  expect(screen.queryByRole('dialog')).toBeNull()
  expect(f.api.refresh).not.toHaveBeenCalled()
})

it('refreshes OS disk usage on launch, focus, and the visible timer without scanning projects', async () => {
  const interval = vi.spyOn(window, 'setInterval')
  const disks = [{ name: 'System', mount: '/', total: 1000, available: 500 }]
  const refreshDisks = vi.fn(async () => disks.map((d) => ({ ...d })))
  const f = await fixture({ refreshDisks })
  await waitFor(() => expect(f.state().data.inventory.disks[0]?.available).toBe(500))
  vi.useFakeTimers()
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000)
    fireEvent(window, new Event('focus'))
  })
  expect(refreshDisks).toHaveBeenCalledTimes(2)
  disks[0].available = 700
  await act(async () => {
    await vi.advanceTimersByTimeAsync(6000)
    fireEvent(document, new Event('visibilitychange'))
  })
  await act(async () => {
    await vi.advanceTimersByTimeAsync(30000)
    const callback = interval.mock.calls.find(([, delay]) => delay === 30000)![0] as () => void
    callback()
  })
  expect(f.state().data.inventory.disks[0].available).toBe(700)
  expect(f.api.refresh).not.toHaveBeenCalled()
  expect(f.api.prepare).not.toHaveBeenCalled()
  f.unmount()
  fireEvent(window, new Event('focus'))
  expect(refreshDisks).toHaveBeenCalledTimes(4)
  interval.mockRestore()
})

it('collapses version-specific dependents and avoids cleanup messaging for runtime operations', async () => {
  const f = await fixture({
    prepare: async () => ({
      ...plan,
      runtimeDependents: [{ path: '/projects/related', pins: ['.node-version: 16'] }],
    }),
    execute: async () => ({
      items: [{ title: 'Remove runtime', status: 'success', message: 'Removed.', removedBytes: 0 }],
      removedBytes: 0,
      reclaimedBytes: null,
      cancelled: false,
    }),
  })
  await act(() => f.state().prepare({ kind: 'removeRuntime', provider: 'js', id: 'runtime' }))
  const summary = screen.getByText('Scanned projects requesting this version (1)')
  expect(summary.closest('details')!.open).toBe(false)
  fireEvent.click(summary)
  expect(screen.getByText('.node-version: 16')).toBeTruthy()
  await act(() => f.state().execute())
  expect(screen.queryByText(/Logical size removed/)).toBeNull()
  expect(screen.queryByText(/Files in Trash/)).toBeNull()
})

it('requires a new review after execution rejects a consumed plan', async () => {
  const prepare = vi
    .fn()
    .mockResolvedValueOnce(plan)
    .mockResolvedValueOnce({ ...plan, id: 'new-plan' })
  const execute = vi
    .fn()
    .mockRejectedValueOnce(new Error('This plan expired. Review the operation again.'))
    .mockResolvedValueOnce({ items: [], removedBytes: 0, reclaimedBytes: null, cancelled: false })
  const f = await fixture({ prepare, execute })
  await act(() => f.state().prepare({ kind: 'cleanCaches', ids: ['cache'] }))
  await act(() => f.state().execute())
  expect(screen.queryByRole('button', { name: 'Confirm operation' })).toBeNull()
  expect(screen.getByRole('alert').textContent).toContain('plan expired')
  expect(f.state().error).toBeNull()
  await act(() => f.state().execute())
  expect(execute).toHaveBeenCalledTimes(1)
  fireEvent.click(screen.getByRole('button', { name: 'Review again' }))
  await waitFor(() => expect(f.state().plan?.id).toBe('new-plan'))
  expect(screen.getByRole('button', { name: 'Confirm operation' })).toBeTruthy()
  await act(() => f.state().execute())
  expect(execute.mock.calls[1][0]).toBe('new-plan')
  expect(screen.getByRole('button', { name: 'Done' })).toBeTruthy()
})

it('keeps actual results when only the post-operation snapshot fails', async () => {
  const execute = vi.fn(async () => ({
    items: [],
    removedBytes: 0,
    reclaimedBytes: null,
    cancelled: false,
  }))
  const f = await fixture({ execute })
  await act(() => f.state().prepare({ kind: 'cleanCaches', ids: ['cache'] }))
  vi.mocked(f.api.snapshot).mockRejectedValueOnce(new Error('Snapshot unavailable'))
  await act(() => f.state().execute())
  expect(f.state().operationError).toBeNull()
  expect(screen.getByRole('button', { name: 'Done' })).toBeTruthy()
  expect(screen.queryByRole('button', { name: 'Review again' })).toBeNull()
  await act(() => f.state().execute())
  expect(execute).toHaveBeenCalledTimes(1)
})

it('clears only successful target IDs rather than inferring IDs from request order', async () => {
  const data = structuredClone(emptySnapshot)
  data.settings.scanOnLaunch = false
  data.settings.language = 'en'
  data.inventory.caches = ['success', 'failure', 'not-attempted'].map((id) => ({
    id,
    provider: 'js',
    name: id,
    path: `/cache/${id}`,
    strategy: 'npm',
    warning: '',
    canClean: true,
    size: { bytes: 100, files: 1, skipped: 0, complete: true },
  }))
  const f = await fixture({
    snapshot: async () => data,
    prepare: async () => ({
      ...plan,
      items: data.inventory.caches.map((cache) => ({
        title: cache.name,
        path: cache.path,
        command: null,
        restore: null,
        bytes: 100,
      })),
    }),
    execute: async () => ({
      items: [
        {
          title: 'success',
          status: 'success',
          message: 'Done',
          removedBytes: 100,
          targetIds: ['success'],
        },
        {
          title: 'failure',
          status: 'cancelled',
          message: 'Interrupted',
          removedBytes: 0,
          targetIds: ['failure'],
        },
      ],
      removedBytes: 100,
      reclaimedBytes: null,
      cancelled: true,
    }),
  })
  await act(() =>
    f.state().prepare({ kind: 'cleanCaches', ids: ['not-attempted', 'failure', 'success'] }),
  )
  await act(() => f.state().execute())
  expect(f.state().completedSelectionIds).toEqual(['success'])
})

it('does not clear failed models sharing the successful model path', async () => {
  const f = await fixture({
    prepare: async () => ({
      ...plan,
      items: ['model-b', 'model-a'].map((title) => ({
        title,
        path: '/shared/models',
        command: null,
        restore: null,
        bytes: 0,
      })),
    }),
    execute: async () => ({
      items: [
        {
          title: 'model-b',
          status: 'success',
          message: 'Removed',
          removedBytes: 0,
          targetIds: ['model-b-id'],
        },
        {
          title: 'model-a',
          status: 'failed',
          message: 'Still in use',
          removedBytes: 0,
          targetIds: ['model-a-id'],
        },
      ],
      cancelled: false,
      removedBytes: 0,
      reclaimedBytes: null,
    }),
  })
  await act(() =>
    f
      .state()
      .prepare({ kind: 'removeAssets', provider: 'ollama', ids: ['model-a-id', 'model-b-id'] }),
  )
  await act(() => f.state().execute())
  expect(f.state().completedSelectionIds).toEqual(['model-b-id'])
})

it('clears successful tool IDs without requiring a filesystem path or an inventory asset', async () => {
  const f = await fixture({
    execute: async () => ({
      items: [
        {
          title: 'Update a tool',
          status: 'success',
          message: 'Updated',
          removedBytes: 0,
          targetIds: ['tool-id'],
        },
        {
          title: 'Update a second tool',
          status: 'failed',
          message: 'Unavailable',
          removedBytes: 0,
          targetIds: ['other-tool'],
        },
      ],
      cancelled: false,
      removedBytes: 0,
      reclaimedBytes: null,
    }),
  })
  await act(() =>
    f.state().prepare({ kind: 'updateTools', provider: 'js', ids: ['other-tool', 'tool-id'] }),
  )
  await act(() => f.state().execute())
  expect(f.state().completedSelectionIds).toEqual(['tool-id'])
})

it('preserves selections when older result payloads omit target IDs', async () => {
  const f = await fixture({
    execute: async () => ({
      items: [{ title: 'Update tool-id', status: 'success', message: 'Updated', removedBytes: 0 }],
      cancelled: false,
      removedBytes: 0,
      reclaimedBytes: null,
    }),
  })
  await act(() => f.state().prepare({ kind: 'updateTools', provider: 'js', ids: ['tool-id'] }))
  await act(() => f.state().execute())
  expect(f.state().completedSelectionIds).toEqual([])
})
