import { expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StoreProvider, useStore } from '@/store'
import { OperationDialog } from '@/components/OperationDialog'
import { emptySnapshot, type Plan } from '@/domain'
import type { Backend } from '@/bridge'

function Launcher() {
  const s = useStore()
  return (
    <>
      <button onClick={() => void s.prepare({ kind: 'removeWorktrees', ids: ['clean', 'dirty'] })}>
        Review worktrees
      </button>
      <OperationDialog />
    </>
  )
}

function fixture(onlyChanged = false) {
  let revision = 0
  const data = structuredClone(emptySnapshot)
  data.settings.scanOnLaunch = false
  const plan: Plan = {
    id: 'review',
    kind: 'removeWorktree',
    createdAt: 0,
    useTrash: true,
    warnings: [],
    items: ['clean', 'dirty'].map((name) => ({
      title: `Remove ${name}`,
      path: `/projects/${name}`,
      bytes: 1024,
      command: null,
      restore: null,
    })),
    worktreeChanges: [
      {
        path: '/projects/dirty',
        files: [
          { path: 'source.ts', originalPath: null, status: ' M' },
          { path: 'new file.txt', originalPath: null, status: '??' },
          { path: 'renamed.ts', originalPath: 'original.ts', status: 'R ' },
        ],
      },
    ],
  }
  const execute = vi.fn(async () => ({
    items: [
      { title: 'Remove dirty', status: 'skipped', message: 'Worktree kept.', removedBytes: 0 },
    ],
    removedBytes: 0,
    cancelled: false,
    reclaimedBytes: null,
  }))
  if (onlyChanged) plan.items = plan.items.filter((item) => item.path === '/projects/dirty')
  const api: Backend = {
    checkToolUpdates: async () => {
      throw new Error('Unexpected update check')
    },
    native: false,
    snapshot: async () => data,
    refresh: async () => data,
    saveSettings: async (settings) => ({ ...data, settings }),
    prepare: async () => ({ ...plan, id: `review-${++revision}` }),
    execute,
    cancel: vi.fn(),
    selectFolder: vi.fn(),
    readConfig: vi.fn(),
    saveConfig: vi.fn(),
    subscribe: async () => () => {},
  }
  render(
    <StoreProvider api={api}>
      <Launcher />
    </StoreProvider>,
  )
  return { execute }
}

it('lists reviewed files and defaults to keeping changed worktrees while removing the rest', async () => {
  const { execute } = fixture()
  const user = userEvent.setup()
  await user.click(screen.getByText('Review worktrees'))
  expect(await screen.findByText('source.ts')).toBeTruthy()
  expect(screen.getByText('new file.txt')).toBeTruthy()
  expect(screen.getByText(/original.ts.*renamed.ts/)).toBeTruthy()
  expect(
    (
      screen.getByRole('radio', {
        name: 'Keep changed worktrees; remove the others',
      }) as HTMLInputElement
    ).checked,
  ).toBe(true)
  await user.click(screen.getByRole('button', { name: 'Remove 1; keep 1' }))
  await waitFor(() => expect(execute).toHaveBeenCalledWith('review-1', expect.any(String), false))
  expect(await screen.findByText('Kept')).toBeTruthy()
})

it('requires a discard choice and resets it when reviewing another plan', async () => {
  const { execute } = fixture()
  const user = userEvent.setup()
  await user.click(screen.getByText('Review worktrees'))
  await user.click(
    await screen.findByRole('radio', {
      name: 'Discard listed changes and remove all selected worktrees',
    }),
  )
  await user.click(screen.getByRole('button', { name: 'Cancel' }))
  expect(execute).not.toHaveBeenCalled()
  await user.click(screen.getByText('Review worktrees'))
  expect(await screen.findByRole('button', { name: 'Remove 1; keep 1' })).toBeTruthy()
  await user.click(
    screen.getByRole('radio', { name: 'Discard listed changes and remove all selected worktrees' }),
  )
  await user.click(screen.getByRole('button', { name: 'Discard changes and remove all' }))
  await waitFor(() => expect(execute).toHaveBeenCalledWith('review-2', expect.any(String), true))
})

it('keeps an all-dirty selection without sending an execute request', async () => {
  const { execute } = fixture(true)
  const user = userEvent.setup()
  await user.click(screen.getByText('Review worktrees'))
  await user.click(await screen.findByRole('button', { name: 'Keep all worktrees' }))
  expect(execute).not.toHaveBeenCalled()
  expect(screen.queryByRole('dialog')).toBeNull()
})
