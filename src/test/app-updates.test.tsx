import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import type { Backend } from '@/bridge'
import type { AppRelease, AppUpdateProgress } from '@/desktop'
import { emptySnapshot } from '@/domain'
import { StoreProvider } from '@/store'
import { AppUpdateProvider, useAppUpdates } from '@/components/AppUpdates'
import { TooltipProvider } from '@/components/ui/tooltip'

function Trigger() {
  const check = useAppUpdates()
  return <button onClick={check}>Check update</button>
}

function fixture(release: AppRelease = { version: '0.3.0', available: true, installable: true }) {
  const snapshot = structuredClone(emptySnapshot)
  snapshot.settings.language = 'en'
  snapshot.settings.scanOnLaunch = false
  let onProgress: ((progress: AppUpdateProgress) => void) | undefined
  const listeners: Array<(progress: AppUpdateProgress) => void> = []
  const stop = vi.fn()
  const install = vi.fn(async () => {})
  const restart = vi.fn(async () => {})
  const check = vi.fn(async () => release)
  const api: Backend = {
    managerOptions: async () => [],
    checkToolUpdates: async () => {
      throw new Error('Unexpected update check')
    },
    native: true,
    snapshot: async () => snapshot,
    refresh: vi.fn(),
    refreshCaches: vi.fn(),
    saveSettings: vi.fn(),
    prepare: vi.fn(),
    execute: vi.fn(),
    cancel: vi.fn(),
    selectFolder: vi.fn(),
    readConfig: vi.fn(),
    saveConfig: vi.fn(),
    subscribe: async () => () => {},
    checkAppUpdate: check,
    installAppUpdate: install,
    restartAfterUpdate: restart,
    subscribeAppUpdate: async (handler) => {
      onProgress = handler
      listeners.push(handler)
      return stop
    },
  }
  render(
    <TooltipProvider>
      <StoreProvider api={api}>
        <AppUpdateProvider>
          <Trigger />
        </AppUpdateProvider>
      </StoreProvider>
    </TooltipProvider>,
  )
  return {
    check,
    install,
    restart,
    stop,
    progress: (value: AppUpdateProgress) => act(() => onProgress?.(value)),
    progressFromAttempt: (attempt: number, value: AppUpdateProgress) =>
      act(() => listeners[attempt]?.(value)),
  }
}

it('downloads signed updates, shows progress, prevents duplicate installs, and restarts explicitly', async () => {
  const { install, restart, stop, progress } = fixture()
  let finish: () => void = () => {}
  install.mockImplementation(
    () =>
      new Promise<void>((resolve) => {
        finish = resolve
      }),
  )
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Check update' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await waitFor(() => expect(install).toHaveBeenCalledOnce())
  progress({ downloaded: 512, total: 1024, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
  progress({ downloaded: 256, total: 1024, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
  progress({ downloaded: 768, total: null, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('75')
  await user.keyboard('{Escape}')
  expect(screen.getByRole('dialog')).toBeTruthy()
  progress({ downloaded: 1024, total: 1024, installing: true })
  expect(screen.getByText('Installing update…')).toBeTruthy()
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('100')
  progress({ downloaded: 800, total: 1024, installing: false })
  expect(screen.getByText('Installing update…')).toBeTruthy()
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('100')
  await act(async () => finish())
  const button = await screen.findByRole('button', { name: 'Restart now' })
  progress({ downloaded: 900, total: 1024, installing: false })
  expect(screen.getByRole('button', { name: 'Restart now' })).toBe(button)
  expect(screen.queryByRole('progressbar')).toBeNull()
  expect(stop).toHaveBeenCalledOnce()
  expect(restart).not.toHaveBeenCalled()
  await user.click(button)
  expect(restart).toHaveBeenCalledOnce()
})

it('keeps unsigned or unsupported releases available only through the manual release link', async () => {
  const { install } = fixture({ version: '0.3.0', available: true, installable: false })
  await userEvent.setup().click(screen.getByRole('button', { name: 'Check update' }))
  await screen.findByText('New version available · 0.3.0')
  expect(screen.queryByRole('button', { name: 'Download and install' })).toBeNull()
  expect(screen.getByRole('link', { name: 'View release page' })).toBeTruthy()
  expect(install).not.toHaveBeenCalled()
})

it('opens immediately during an update check and does not reopen on a late response', async () => {
  const { check } = fixture()
  let resolve!: (release: AppRelease) => void
  check.mockImplementationOnce(
    () =>
      new Promise((done) => {
        resolve = done
      }),
  )
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Check update' }))
  expect(screen.getByRole('dialog', { name: 'Check for Envark updates' })).toBeTruthy()
  expect(screen.getByRole('status').textContent).toContain('Checking the latest release')
  await user.keyboard('{Escape}')
  expect(screen.queryByRole('dialog')).toBeNull()
  await act(async () => resolve({ version: '0.4.0', available: true, installable: true }))
  expect(screen.queryByRole('dialog')).toBeNull()
})

it('cleans up the progress listener and permits retry after an installation failure', async () => {
  const { check, install, stop } = fixture()
  install.mockRejectedValueOnce(new Error('Signature verification failed'))
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Check update' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await screen.findByText('Error: Signature verification failed')
  expect(stop).toHaveBeenCalledOnce()
  await user.click(screen.getByRole('button', { name: 'Retry' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await screen.findByRole('button', { name: 'Restart now' })
  expect(check).toHaveBeenCalledTimes(2)
  expect(install).toHaveBeenCalledTimes(2)
})

it('shows downloaded bytes without inventing a percentage when the server omits the length', async () => {
  const { install, progress } = fixture()
  let finish!: () => void
  install.mockImplementation(() => new Promise<void>((resolve) => (finish = resolve)))
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Check update' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await waitFor(() => expect(install).toHaveBeenCalledOnce())
  progress({ downloaded: 4096, total: null, installing: false })
  expect(screen.getByText('4.0 KB')).toBeTruthy()
  expect(screen.queryByRole('progressbar')).toBeNull()
  progress({ downloaded: 2048, total: 0, installing: false })
  expect(screen.getByText('4.0 KB')).toBeTruthy()
  expect(screen.queryByRole('progressbar')).toBeNull()
  progress({ downloaded: 4096, total: 8192, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('50')
  progress({ downloaded: 8192, total: 8192, installing: true })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('100')
  await act(async () => finish())
})

it('starts a retried download at zero and keeps late failed-attempt events out of its progress', async () => {
  const { install, progress, progressFromAttempt } = fixture()
  let fail!: (error: Error) => void
  let finish!: () => void
  install
    .mockImplementationOnce(() => new Promise<void>((_, reject) => (fail = reject)))
    .mockImplementationOnce(() => new Promise<void>((resolve) => (finish = resolve)))
  const user = userEvent.setup()
  await user.click(screen.getByRole('button', { name: 'Check update' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await waitFor(() => expect(install).toHaveBeenCalledOnce())
  progress({ downloaded: 768, total: 1024, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('75')
  await act(async () => fail(new Error('Download interrupted')))
  progress({ downloaded: 1024, total: 1024, installing: true })
  expect(screen.getByText('Error: Download interrupted')).toBeTruthy()
  await user.click(screen.getByRole('button', { name: 'Retry' }))
  await user.click(await screen.findByRole('button', { name: 'Download and install' }))
  await waitFor(() => expect(install).toHaveBeenCalledTimes(2))
  expect(screen.queryByRole('progressbar')).toBeNull()
  progress({ downloaded: 256, total: 1024, installing: false })
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('25')
  progressFromAttempt(0, { downloaded: 1024, total: 1024, installing: true })
  expect(screen.getByText('Downloading update…')).toBeTruthy()
  expect(screen.getByRole('progressbar').getAttribute('aria-valuenow')).toBe('25')
  await act(async () => finish())
})
