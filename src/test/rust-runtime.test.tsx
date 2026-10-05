import { act, render, screen, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { expect, it, vi } from 'vitest'
import App from '@/App'
import { backend } from '@/bridge'
import { emptyProvider, emptySnapshot } from '@/domain'

it.each([
  ['1.99.0', 'stable-x86_64-pc-windows-msvc'],
  ['1.100.0-nightly', 'nightly-aarch64-apple-darwin'],
])('shows Rust %s consistently in the overview and sidebar', async (version, selector) => {
  const data = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  const rust = emptyProvider('rust')
  rust.detected = true
  rust.runtimes = [
    {
      id: 'rust-runtime',
      version,
      selector,
      manager: 'rustup',
      path: `/toolchains/${selector}`,
      active: true,
      activeKnown: true,
      managed: true,
      size: null,
      note: null,
    },
  ]
  data.inventory.providers = [rust]
  await act(async () =>
    render(<App api={{ ...backend, native: false, snapshot: async () => data }} />),
  )
  await act(() => vi.dynamicImportSettled())
  const main = screen.getByRole('main')
  const runtime = within(main).getByText('Rust').closest('button')!
  expect(within(runtime).getByText(version)).toBeTruthy()
  expect(screen.queryByText('Toolchain')).toBeNull()
  const sidebar = screen.getByRole('button', { name: 'Rust' }).closest('li')!
  expect(within(sidebar).getByText(version).getAttribute('title')).toBe(version)

  await userEvent.setup().click(runtime)
  await act(() => vi.dynamicImportSettled())
  expect(within(main).getByText(version)).toBeTruthy()
  expect(screen.getByText(selector)).toBeTruthy()
})
