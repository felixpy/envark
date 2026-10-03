export type ProviderId = 'js' | 'py' | 'jvm' | 'rust' | 'go' | 'ollama' | 'puppeteer' | 'playwright'
export type View = 'overview' | 'env' | 'projects' | 'caches' | 'activity' | 'settings'
export interface Settings {
  language: 'zh-CN' | 'zh-TW' | 'en'
  theme: 'light' | 'dark' | 'system'
  scanOnLaunch: boolean
  checkUpdates: boolean
  roots: string[]
  excludes: string[]
  idleDays: number
  useTrash: boolean
  preferred: Partial<Record<ProviderId, string>>
  protectedProjects: string[]
}
export interface Measurement {
  bytes: number
  files: number
  skipped: number
  complete: boolean
}
export interface Runtime {
  id: string
  version: string
  manager: string
  path: string
  active: boolean
  managed: boolean
  size: Measurement | null
  note: string | null
}
export interface Manager {
  name: string
  version: string
  path: string
  supportsInstall: boolean
  supportsDefault: boolean
}
export interface Tool {
  id: string
  name: string
  version: string
  latest: string | null
  source: string
  runtime: string | null
  path: string | null
  size: Measurement | null
  canUpdate: boolean
  canRemove: boolean
  note: string | null
}
export interface Asset {
  id: string
  name: string
  version: string
  path: string
  size: Measurement
  lastUsed: number | null
  modified: number | null
  usedBy: string[]
  canRemove: boolean
  note: string | null
}
export interface ConfigFile {
  id: string
  path: string
  format: string
  editable: boolean
  warning: string | null
}
export interface ConfigContent {
  id: string
  content: string
  revision: string
  path: string
}
export interface Provider {
  id: ProviderId
  detected: boolean
  managers: Manager[]
  runtimes: Runtime[]
  packageManagers: Tool[]
  tools: Tool[]
  assets: Asset[]
  configs: ConfigFile[]
  service: { running: boolean; owned: boolean; endpoint: string } | null
  issues: string[]
}
export interface Artifact {
  id: string
  name: string
  path: string
  kind: string
  size: Measurement
  restore: string
}
export interface Project {
  id: string
  name: string
  path: string
  providers: ProviderId[]
  lastActive: number | null
  activityComplete: boolean
  branch: string | null
  pins: Record<string, string>
  protected: boolean
  artifacts: Artifact[]
}
export interface Cache {
  id: string
  provider: ProviderId
  name: string
  path: string
  size: Measurement
  strategy: string
  warning: string
  canClean: boolean
}
export interface Disk {
  name: string
  mount: string
  total: number
  available: number
}
export interface Inventory {
  providers: Provider[]
  projects: Project[]
  caches: Cache[]
  disks: Disk[]
  scannedAt: number | null
  issues: string[]
}
export interface Activity {
  id: string
  time: number
  kind: string
  title: string
  status: string
  detail: string
  freedBytes: number
}
export interface Snapshot {
  settings: Settings
  inventory: Inventory
  activity: Activity[]
  dataDir: string
  platform: string
  version: string
}
export interface Progress {
  jobId: string
  stage: string
  completed: number
  total: number | null
  message: string
}
export type ActionRequest =
  | { kind: 'cleanProjects'; artifactIds: string[] }
  | { kind: 'cleanCaches'; ids: string[] }
  | { kind: 'installRuntime'; provider: ProviderId; manager: string; version: string }
  | {
      kind: 'setDefault' | 'removeRuntime' | 'updateTool' | 'removeTool'
      provider: ProviderId
      id: string
    }
  | { kind: 'removeAssets'; provider: ProviderId; ids: string[] }
  | { kind: 'downloadAsset'; provider: ProviderId; name: string }
export interface Plan {
  id: string
  kind: string
  createdAt: number
  items: {
    title: string
    path: string | null
    command: string | null
    bytes: number
    restore: string | null
  }[]
  warnings: string[]
  useTrash: boolean
}
export interface OperationResult {
  items: { title: string; status: string; message: string; freedBytes: number }[]
  cancelled: boolean
  freedBytes: number
}

export const defaultSettings: Settings = {
  language: 'zh-CN',
  theme: 'system',
  scanOnLaunch: true,
  checkUpdates: false,
  roots: [],
  excludes: ['.git', '.svn', '.hg'],
  idleDays: 90,
  useTrash: true,
  preferred: { js: 'fnm', py: 'uv' },
  protectedProjects: [],
}
export const emptySnapshot: Snapshot = {
  settings: defaultSettings,
  inventory: { providers: [], projects: [], caches: [], disks: [], scannedAt: null, issues: [] },
  activity: [],
  dataDir: '',
  platform: '',
  version: '0.1.0',
}

export const metadata = {
  js: {
    name: 'JavaScript / TypeScript',
    short: 'JavaScript',
    color: '#eab308',
    category: 'lang',
    runtime: 'Node.js',
    description: [
      '多版本 Node 共存，统一查看包管理器与全局工具。',
      'Node versions, package managers, and global tools in one place.',
    ],
  },
  py: {
    name: 'Python',
    short: 'Python',
    color: '#3b82f6',
    category: 'lang',
    runtime: 'CPython',
    description: [
      '管理解释器、独立工具和虚拟环境。',
      'Manage interpreters, isolated tools, and virtual environments.',
    ],
  },
  jvm: {
    name: 'Java / JVM',
    short: 'Java',
    color: '#ef4444',
    category: 'lang',
    runtime: 'JDK',
    description: [
      '查看 JDK 发行版、构建工具与依赖缓存。',
      'Inspect JDK distributions, build tools, and dependency caches.',
    ],
  },
  rust: {
    name: 'Rust',
    short: 'Rust',
    color: '#f97316',
    category: 'lang',
    runtime: 'Toolchain',
    description: [
      '管理 rustup 工具链、Cargo 工具和构建产物。',
      'Manage rustup toolchains, Cargo tools, and build artifacts.',
    ],
  },
  go: {
    name: 'Go',
    short: 'Go',
    color: '#06b6d4',
    category: 'lang',
    runtime: 'Go SDK',
    description: [
      '查看 Go SDK、安装的命令行工具和模块缓存。',
      'Inspect Go SDKs, installed commands, and module caches.',
    ],
  },
  ollama: {
    name: 'Ollama',
    short: 'Ollama',
    color: '#a855f7',
    category: 'ai',
    runtime: 'Ollama',
    description: [
      '管理本地模型、下载资源与后台服务。',
      'Manage local models, downloads, and the background service.',
    ],
  },
  puppeteer: {
    name: 'Puppeteer',
    short: 'Puppeteer',
    color: '#10b981',
    category: 'browser',
    runtime: 'Puppeteer',
    description: [
      '检查 Chrome 浏览器下载与共享资源。',
      'Inspect downloaded Chrome browsers and shared resources.',
    ],
  },
  playwright: {
    name: 'Playwright',
    short: 'Playwright',
    color: '#22c55e',
    category: 'browser',
    runtime: 'Playwright',
    description: [
      '检查 Chromium、Firefox、WebKit 下载资源。',
      'Inspect Chromium, Firefox, and WebKit browser downloads.',
    ],
  },
} as const
export const providerIds = Object.keys(metadata) as ProviderId[]
export const categories = [
  { id: 'lang', zh: '编程语言', en: 'Languages' },
  { id: 'ai', zh: '本地 AI', en: 'Local AI' },
  { id: 'browser', zh: '浏览器与自动化', en: 'Browsers & automation' },
]
export function emptyProvider(id: ProviderId): Provider {
  return {
    id,
    detected: false,
    managers: [],
    runtimes: [],
    packageManagers: [],
    tools: [],
    assets: [],
    configs: [],
    service: null,
    issues: [],
  }
}
export function formatBytes(bytes: number) {
  if (bytes === 0) return '0 B'
  const unit = Math.min(4, Math.floor(Math.log(bytes) / Math.log(1024)))
  return `${(bytes / 1024 ** unit).toFixed(unit > 0 && bytes / 1024 ** unit < 10 ? 1 : 0)} ${['B', 'KB', 'MB', 'GB', 'TB'][unit]}`
}
export function idle(project: Project, days: number, time = Date.now()) {
  return (
    project.activityComplete &&
    project.lastActive !== null &&
    time / 1000 - project.lastActive > days * 86_400
  )
}
export function projectBytes(project: Project) {
  return project.artifacts.reduce((sum, artifact) => sum + artifact.size.bytes, 0)
}
export function updateKind(tool: Tool): 'unknown' | 'latest' | 'major' | 'minor' {
  if (!tool.latest) return 'unknown'
  if (tool.latest === tool.version) return 'latest'
  return Number(tool.latest.replace(/^v/, '').split('.')[0]) >
    Number(tool.version.replace(/^v/, '').split('.')[0])
    ? 'major'
    : 'minor'
}
