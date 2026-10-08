import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { toast } from 'sonner'
import traditionalStrings from './locales/zh-TW.json'
import { backend, type Backend } from './bridge'
import {
  emptySnapshot,
  type ActionRequest,
  type OperationResult,
  type Plan,
  type Progress,
  type ProviderId,
  type Settings,
  type Snapshot,
  type View,
  type NavigationFocus,
} from './domain'

interface Store {
  data: Snapshot
  loaded: boolean
  error: string | null
  api: Backend
  view: View
  provider: ProviderId | null
  focus: NavigationFocus
  navigationKey: number
  go(view: View, provider?: ProviderId | null, focus?: NavigationFocus): void
  busy: boolean
  progress: Progress | null
  cancel(): Promise<void>
  refresh(): Promise<void>
  saveSettings(settings: Settings): Promise<boolean>
  setTheme(theme: Settings['theme']): Promise<void>
  setDisabledShortcuts(disabled: Settings['disabledShortcuts']): Promise<void>
  addRoot(path?: string): Promise<void>
  plan: Plan | null
  result: OperationResult | null
  prepare(request: ActionRequest): Promise<void>
  execute(discardWorktreeChanges?: boolean): Promise<void>
  closePlan(): void
  reload(): Promise<void>
  t(zh: string, en: string): string
}
const Context = createContext<Store | null>(null)
const traditional = (value: string) =>
  (traditionalStrings as Record<string, string>)[value] ?? value
export function useStore() {
  const store = useContext(Context)
  if (!store) throw new Error('Store provider is missing')
  return store
}

export function StoreProvider({ children, api = backend }: { children: ReactNode; api?: Backend }) {
  const [data, setData] = useState(emptySnapshot)
  const [loaded, setLoaded] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [view, setView] = useState<View>('overview')
  const [provider, setProvider] = useState<ProviderId | null>(null)
  const [focus, setFocus] = useState<NavigationFocus>({})
  const [navigationKey, setNavigationKey] = useState(0)
  const [job, setJob] = useState<string | null>(null)
  const [progress, setProgress] = useState<Progress | null>(null)
  const [plan, setPlan] = useState<Plan | null>(null)
  const [result, setResult] = useState<OperationResult | null>(null)
  const launched = useRef(false)
  const running = useRef(false)
  const t = (zh: string, en: string) =>
    data.settings.language === 'en' ? en : data.settings.language === 'zh-TW' ? traditional(zh) : zh
  const fail = (cause: unknown) => {
    const message = String(cause)
    setError(message)
    toast.error(message)
  }
  const refresh = async () => {
    if (running.current) return
    running.current = true
    const id = crypto.randomUUID()
    setJob(id)
    setProgress(null)
    setError(null)
    try {
      setData(await api.refresh(id))
    } catch (cause) {
      fail(cause)
    } finally {
      running.current = false
      setJob(null)
      setProgress(null)
    }
  }
  useEffect(() => {
    let active = true
    let unsubscribe: (() => void) | undefined
    void api
      .subscribe((event) => {
        if (active) setProgress(event)
      })
      .then((stop) => {
        if (active) unsubscribe = stop
        else stop()
      })
      .catch(fail)
    void api
      .snapshot()
      .then((snapshot) => {
        if (!active) return
        setData(snapshot)
        setLoaded(true)
        if (!launched.current && snapshot.settings.scanOnLaunch && api.native) {
          launched.current = true
          void refresh()
        }
      })
      .catch((cause) => {
        if (active) {
          fail(cause)
          setLoaded(true)
        }
      })
    return () => {
      active = false
      unsubscribe?.()
    }
  }, [api])
  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const apply = () =>
      document.documentElement.classList.toggle(
        'dark',
        data.settings.theme === 'dark' || (data.settings.theme === 'system' && media.matches),
      )
    apply()
    media.addEventListener('change', apply)
    document.documentElement.lang = data.settings.language
    return () => media.removeEventListener('change', apply)
  }, [data.settings.theme, data.settings.language])
  const saveSettings = async (settings: Settings) => {
    try {
      setData(await api.saveSettings(settings))
      return true
    } catch (cause) {
      fail(cause)
      return false
    }
  }
  const store: Store = {
    data,
    loaded,
    error,
    api,
    view,
    provider,
    focus,
    navigationKey,
    busy: job !== null,
    progress,
    plan,
    result,
    t,
    go: (next, id, nextFocus = {}) => {
      setView(next)
      setProvider(id ?? null)
      setFocus(nextFocus)
      setNavigationKey((key) => key + 1)
    },
    refresh,
    saveSettings,
    setTheme: async (theme) => {
      try {
        const snapshot = api.setTheme
          ? await api.setTheme(theme)
          : await api.saveSettings({ ...data.settings, theme })
        setData(snapshot)
      } catch (cause) {
        fail(cause)
      }
    },
    setDisabledShortcuts: async (disabledShortcuts) => {
      try {
        const snapshot = api.setDisabledShortcuts
          ? await api.setDisabledShortcuts(disabledShortcuts)
          : await api.saveSettings({ ...data.settings, disabledShortcuts })
        setData(snapshot)
      } catch (cause) {
        fail(cause)
      }
    },
    cancel: async () => {
      if (job) await api.cancel(job)
    },
    addRoot: async (path) => {
      if (running.current) return
      try {
        const root = path ?? (await api.selectFolder())
        if (root && !data.settings.roots.includes(root)) {
          const saved = await saveSettings({
            ...data.settings,
            roots: [...data.settings.roots, root],
          })
          if (saved) await refresh()
        }
      } catch (cause) {
        fail(cause)
      }
    },
    prepare: async (request) => {
      if (running.current) return
      try {
        setResult(null)
        setPlan(await api.prepare(request))
      } catch (cause) {
        fail(cause)
      }
    },
    execute: async (discardWorktreeChanges = false) => {
      if (!plan || running.current) return
      running.current = true
      const id = crypto.randomUUID()
      setJob(id)
      setError(null)
      try {
        setResult(await api.execute(plan.id, id, discardWorktreeChanges))
        setData(await api.snapshot())
      } catch (cause) {
        fail(cause)
      } finally {
        running.current = false
        setJob(null)
        setProgress(null)
      }
    },
    closePlan: () => {
      if (!job) {
        setPlan(null)
        setResult(null)
      }
    },
    reload: async () => {
      setData(await api.snapshot())
    },
  }
  return <Context.Provider value={store}>{children}</Context.Provider>
}
