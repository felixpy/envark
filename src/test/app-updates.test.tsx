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
  await user.keyboard('{Escape}')
  expect(screen.getByRole('dialog')).toBeTruthy()
  progress({ downloaded: 1024, total: 1024, installing: true })
  expect(screen.getByText('Installing update…')).toBeTruthy()
  await act(async () => finish())
  const button = await screen.findByRole('button', { name: 'Restart now' })
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
