import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  ActionRequest,
  ConfigContent,
  OperationResult,
  Plan,
  Progress,
  Settings,
  Snapshot,
} from './domain'
import { emptyProvider, emptySnapshot, providerIds } from './domain'

export interface Backend {
  native: boolean
  snapshot(): Promise<Snapshot>
  refresh(jobId: string): Promise<Snapshot>
  saveSettings(settings: Settings): Promise<Snapshot>
  prepare(request: ActionRequest): Promise<Plan>
  execute(planId: string, jobId: string): Promise<OperationResult>
  cancel(jobId: string): Promise<void>
  selectFolder(): Promise<string | null>
  readConfig(id: string): Promise<ConfigContent>
  saveConfig(id: string, content: string, revision: string): Promise<ConfigContent>
  subscribe(callback: (progress: Progress) => void): Promise<() => void>
  subscribeMenu?(callback: (action: string) => void): Promise<() => void>
}

const native: Backend = {
  native: true,
  snapshot: () => invoke('snapshot'),
  refresh: (jobId) => invoke('refresh', { jobId }),
  saveSettings: (settings) => invoke('save_settings', { settings }),
  prepare: (request) => invoke('prepare_operation', { request }),
  execute: (planId, jobId) => invoke('execute_operation', { planId, jobId }),
  cancel: (jobId) => invoke('cancel', { jobId }),
  selectFolder: async () => {
    const result = await open({ directory: true, multiple: false })
    return typeof result === 'string' ? result : null
  },
  readConfig: (id) => invoke('read_config', { id }),
  saveConfig: (id, content, revision) => invoke('save_config', { id, content, revision }),
  subscribe: (callback) =>
    listen<Progress>('envark://progress', (event) => callback(event.payload)),
  subscribeMenu: (callback) => listen<string>('envark://menu', (event) => callback(event.payload)),
}

function browserBackend(): Backend {
  let state: Snapshot = {
    ...structuredClone(emptySnapshot),
    platform: 'browser',
    inventory: { ...emptySnapshot.inventory, providers: providerIds.map(emptyProvider) },
  }
  const unavailable = async (): Promise<never> => {
    throw new Error('Open the desktop application to inspect or modify this computer.')
  }
  return {
    native: false,
    snapshot: async () => structuredClone(state),
    refresh: unavailable,
    prepare: unavailable,
    execute: unavailable,
    readConfig: unavailable,
    saveConfig: unavailable,
    cancel: async () => {},
    selectFolder: unavailable,
    subscribe: async () => () => {},
    saveSettings: async (settings) => {
      state = { ...state, settings }
      return structuredClone(state)
    },
  }
}

export const backend = isTauri() ? native : browserBackend()
