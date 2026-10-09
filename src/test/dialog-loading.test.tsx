import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import { backend, type Backend } from '@/bridge'
import {
  emptyProvider,
  emptySnapshot,
  type ConfigContent,
  type ManagerInstallOption,
} from '@/domain'
import { StoreProvider } from '@/store'
import { TooltipProvider } from '@/components/ui/tooltip'
import Environments from '@/views/Environments'

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: Error) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

function fixture(overrides: Partial<Backend>) {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  const provider = emptyProvider('js')
  provider.configs = ['first', 'second'].map((id) => ({
    id,
    path: `/example/${id}.npmrc`,
    format: 'ini',
    editable: true,
    warning: null,
  }))
  data.inventory.providers = [provider]
  const snapshot = vi.fn(async () => data)
  render(
    <TooltipProvider>
      <StoreProvider api={{ ...backend, native: false, snapshot, ...overrides }}>
        <Environments id="js" />
      </StoreProvider>
    </TooltipProvider>,
  )
  return { snapshot }
}

const content = (id: string): ConfigContent => ({
  id,
  path: `/example/${id}.npmrc`,
  content: `registry=${id}`,
  revision: id,
})

it('distinguishes a saved configuration from a failed follow-up refresh', async () => {
  const { snapshot } = fixture({
    readConfig: async () => content('first'),
    saveConfig: async () => ({ ...content('first'), revision: 'saved' }),
  })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Configuration' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[0])
  await screen.findByRole('textbox')
  snapshot.mockRejectedValueOnce(new Error('Refresh unavailable'))
  await user.click(screen.getByRole('button', { name: 'Back up & save' }))
  expect(screen.getByRole('alert').textContent).toContain(
    'Configuration saved, but refreshing the view failed',
  )
})

it('opens configuration immediately and loads its content inside the same dialog', async () => {
  const read = deferred<ConfigContent>()
  fixture({ readConfig: () => read.promise })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Configuration' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[0])
  const dialog = screen.getByRole('dialog', { name: 'Edit configuration' })
  expect(within(dialog).getByRole('status').textContent).toContain('Reading configuration')
  expect(screen.queryByRole('textbox')).toBeNull()
  await act(async () => read.resolve(content('first')))
  expect(screen.getByRole('dialog')).toBe(dialog)
  expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('registry=first')
})

it('ignores an old configuration response after closing and opening another file', async () => {
  const first = deferred<ConfigContent>()
  const second = deferred<ConfigContent>()
  fixture({ readConfig: (id) => (id === 'first' ? first.promise : second.promise) })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Configuration' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[0])
  await user.click(screen.getByRole('button', { name: 'Close' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[1])
  await act(async () => second.resolve(content('second')))
  await act(async () => first.resolve(content('first')))
  expect((screen.getByRole('textbox') as HTMLTextAreaElement).value).toBe('registry=second')
  await user.click(screen.getByRole('button', { name: 'Close' }))
  expect(screen.queryByRole('dialog')).toBeNull()
})

it('keeps configuration read errors in the dialog and offers retry', async () => {
  const readConfig = vi
    .fn()
    .mockRejectedValueOnce(new Error('Read denied'))
    .mockResolvedValueOnce(content('first'))
  fixture({ readConfig })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Configuration' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[0])
  expect(await screen.findByRole('alert')).toHaveProperty('textContent', 'Error: Read denied')
  await user.click(screen.getByRole('button', { name: 'Retry' }))
  expect(((await screen.findByRole('textbox')) as HTMLTextAreaElement).value).toBe('registry=first')
})

it('keeps the current manager request loading when an older closed request finishes', async () => {
  const first = deferred<ManagerInstallOption[]>()
  const second = deferred<ManagerInstallOption[]>()
  const managerOptions = vi
    .fn()
    .mockReturnValueOnce(first.promise)
    .mockReturnValueOnce(second.promise)
  fixture({ managerOptions })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('button', { name: 'Install manager' }))
  expect(screen.getByRole('status').textContent).toContain('Checking installation prerequisites')
  await user.click(screen.getByRole('button', { name: 'Close' }))
  await user.click(screen.getByRole('button', { name: 'Install manager' }))
  await act(async () => first.reject(new Error('Stale request error')))
  expect(screen.queryByText('Error: Stale request error')).toBeNull()
  expect(screen.getByRole('status').textContent).toContain('Checking installation prerequisites')
  await act(async () =>
    second.resolve([
      {
        name: 'fnm',
        source: 'official-installer',
        available: true,
        installed: false,
        reason: null,
      },
    ]),
  )
  expect(screen.getByRole('button', { name: 'Install' })).toBeTruthy()
})

it('keeps edited configuration and shows save failures without allowing dismissal during the write', async () => {
  const save = deferred<ConfigContent>()
  fixture({ readConfig: async () => content('first'), saveConfig: () => save.promise })
  const user = userEvent.setup()
  await user.click(await screen.findByRole('tab', { name: 'Configuration' }))
  await user.click(screen.getAllByRole('button', { name: 'View & edit' })[0])
  const editor = (await screen.findByRole('textbox')) as HTMLTextAreaElement
  await user.type(editor, '/edited')
  await user.click(screen.getByRole('button', { name: 'Back up & save' }))
  expect(editor.readOnly).toBe(true)
  expect(screen.queryByRole('button', { name: 'Close' })).toBeNull()
  await user.keyboard('{Escape}')
  expect(screen.getByRole('dialog')).toBeTruthy()
  await act(async () => save.reject(new Error('External modification detected')))
  expect(screen.getByRole('alert').textContent).toContain('External modification detected')
  expect(editor.value).toBe('registry=first/edited')
  expect(editor.readOnly).toBe(false)
  expect(screen.getByRole('button', { name: 'Back up & save' })).toBeTruthy()
})
