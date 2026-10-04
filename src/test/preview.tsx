import { createRoot } from 'react-dom/client'
import '@fontsource-variable/geist'
import '@/index.css'
import App from '@/App'
import { backend, type Backend } from '@/bridge'
import {
  emptyProvider,
  emptySnapshot,
  providerIds,
  type Measurement,
  type Snapshot,
} from '@/domain'

// This development-only entry point is not an input to the production build.
const gib = 1024 ** 3
const size = (bytes: number): Measurement => ({ bytes, files: 250, skipped: 0, complete: true })
const now = Date.now() / 1000
let data: Snapshot = {
  ...structuredClone(emptySnapshot),
  platform: 'test fixtures',
  settings: {
    ...emptySnapshot.settings,
    language: 'en',
    theme: 'light',
    scanOnLaunch: false,
    roots: ['/work'],
  },
  inventory: {
    providers: providerIds.map(emptyProvider),
    projects: ['archive-web', 'active-api', 'protected-service'].map((name, index) => ({
      id: name,
      name,
      path: `/work/${name}`,
      providers: ['js'],
      lastActive: now - [120, 3, 200][index] * 86400,
      activityComplete: true,
      branch: 'main',
      pins: { '.node-version': '24' },
      protected: index === 2,
      artifacts: [
        {
          id: `${name}-modules`,
          name: 'node_modules',
          path: `/work/${name}/node_modules`,
          kind: 'dependencies',
          size: size([2.4, 0.6, 1.2][index] * gib),
          restore: 'pnpm install',
          canClean: true,
          cleanupIssue: null,
        },
      ],
    })),
    caches: [
      {
        id: 'npm-cache',
        provider: 'js',
        name: 'npm cache',
        path: '/example/.npm',
        size: size(1.8 * gib),
        strategy: 'npm cache clean --force',
        warning: 'Packages will be downloaded again when needed.',
        canClean: true,
      },
    ],
    disks: [{ name: 'Example disk', mount: '/', total: 512 * gib, available: 160 * gib }],
    scannedAt: now,
    issues: [],
  },
}
const js = data.inventory.providers.find((p) => p.id === 'js')!
js.detected = true
js.managers = [
  {
    name: 'fnm',
    version: 'test',
    path: '/example/bin/fnm',
    supportsInstall: true,
    supportsDefault: true,
  },
]
js.runtimes = ['24.0.0', '22.0.0'].map((version, index) => ({
  id: version,
  version,
  manager: 'fnm',
  path: `/example/fnm/${version}`,
  active: index === 0,
  managed: true,
  size: null,
  note: null,
}))
js.tools = ['typescript', 'eslint', '@example/linked-tool'].map((name, index) => ({
  id: name,
  name,
  version: '1.0.0',
  latest: index === 2 ? null : '2.0.0',
  source: 'npm',
  runtime: '24.0.0',
  path: `/example/global/node_modules/${name}`,
  size: null,
  canUpdate: index < 2,
  canRemove: index < 2,
  note: index === 2 ? 'Ownership is unknown.' : null,
}))

const api: Backend = {
  ...backend,
  native: false,
  snapshot: async () => structuredClone(data),
  saveSettings: async (settings) => {
    data = { ...data, settings }
    return structuredClone(data)
  },
}

createRoot(document.getElementById('root')!).render(
  <>
    <div
      role="status"
      className="fixed bottom-3 right-3 z-50 rounded-md bg-amber-100 px-3 py-2 text-xs text-amber-950 shadow"
    >
      UI test fixtures · Synthetic data · Host operations unavailable
    </div>
    <App api={api} />
  </>,
)
