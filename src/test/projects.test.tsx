import { describe, expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { StoreProvider } from '@/store'
import Projects from '@/views/Projects'
import { emptySnapshot, type Project, type Snapshot } from '@/domain'
import type { Backend } from '@/bridge'

function project(id: string, protectedProject: boolean, ageDays: number): Project {
  return {
    id,
    name: id,
    path: `/projects/${id}`,
    providers: ['js'],
    lastActive: Date.now() / 1000 - ageDays * 86400,
    activityComplete: true,
    branch: 'main',
    pins: {},
    protected: protectedProject,
    artifacts: [
      {
        id: `${id}-modules`,
        name: 'node_modules',
        path: `/projects/${id}/node_modules`,
        kind: 'dependencies',
        size: { bytes: 1024, files: 1, skipped: 0, complete: true },
        restore: 'pnpm install',
      },
    ],
  }
}

function fixture() {
  const data: Snapshot = structuredClone(emptySnapshot)
  data.settings.language = 'en'
  data.settings.scanOnLaunch = false
  data.settings.roots = ['/projects']
  data.settings.idleDays = 30
  data.inventory.projects = [
    project('older', false, 45),
    project('recent', false, 5),
    project('protected', true, 60),
  ]
  const prepare = vi.fn(async () => ({
    id: 'plan',
    kind: 'clean',
    createdAt: Date.now() / 1000,
    items: [],
    warnings: [],
    useTrash: true,
  }))
  const api: Backend = {
    native: false,
    snapshot: async () => data,
    refresh: async () => data,
    saveSettings: async (settings) => ({ ...data, settings }),
    prepare,
    execute: vi.fn(),
    cancel: vi.fn(),
    selectFolder: vi.fn(),
    readConfig: vi.fn(),
    saveConfig: vi.fn(),
    subscribe: async () => () => {},
  }
  render(
    <StoreProvider api={api}>
      <Projects />
    </StoreProvider>,
  )
  return { prepare }
}

describe('project cleanup selection', () => {
  it('excludes protected projects from bulk cleanup and passes only artifact IDs', async () => {
    const { prepare } = fixture()
    const user = userEvent.setup()
    await screen.findByText('older')
    await user.click(screen.getByRole('checkbox', { name: 'Select all eligible artifacts' }))
    await user.click(screen.getByRole('button', { name: 'Review cleanup' }))
    await waitFor(() =>
      expect(prepare).toHaveBeenCalledWith({
        kind: 'cleanProjects',
        artifactIds: ['older-modules', 'recent-modules'],
      }),
    )
    expect(
      (screen.getByRole('checkbox', { name: 'Select protected' }) as HTMLButtonElement).disabled,
    ).toBe(true)
  })
  it('uses the saved inactivity threshold, not the prototype’s fixed 90 days', async () => {
    fixture()
    const user = userEvent.setup()
    await screen.findByText('older')
    await user.click(screen.getByRole('switch'))
    expect(screen.queryByText('recent')).toBeNull()
    expect(screen.getByText('older')).toBeTruthy()
  })
})
