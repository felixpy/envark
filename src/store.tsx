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
} from './domain'

interface Store {
  data: Snapshot
  loaded: boolean
  error: string | null
  api: Backend
  view: View
  provider: ProviderId | null
  go(view: View, provider?: ProviderId | null): void
  busy: boolean
  progress: Progress | null
  cancel(): Promise<void>
  refresh(): Promise<void>
  saveSettings(settings: Settings): Promise<boolean>
  addRoot(): Promise<void>
  plan: Plan | null
  result: OperationResult | null
  prepare(request: ActionRequest): Promise<void>
  execute(): Promise<void>
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
  const [job, setJob] = useState<string | null>(null)
  const [progress, setProgress] = useState<Progress | null>(null)
  const [plan, setPlan] = useState<Plan | null>(null)
  const [result, setResult] = useState<OperationResult | null>(null)
  const launched = useRef(false)
  const t = (zh: string, en: string) =>
    data.settings.language === 'en' ? en : data.settings.language === 'zh-TW' ? traditional(zh) : zh
  const fail = (cause: unknown) => {
    const message = String(cause)
    setError(message)
    toast.error(message)
  }
  const refresh = async () => {
    const id = crypto.randomUUID()
    setJob(id)
    setProgress(null)
    setError(null)
    try {
      setData(await api.refresh(id))
    } catch (cause) {
      fail(cause)
    } finally {
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
    busy: job !== null,
    progress,
    plan,
    result,
    t,
    go: (next, id) => {
      setView(next)
      if (id !== undefined) setProvider(id)
    },
    refresh,
    saveSettings,
    cancel: async () => {
      if (job) await api.cancel(job)
    },
    addRoot: async () => {
      try {
        const root = await api.selectFolder()
        if (root && !data.settings.roots.includes(root))
          await saveSettings({ ...data.settings, roots: [...data.settings.roots, root] })
      } catch (cause) {
        fail(cause)
      }
    },
    prepare: async (request) => {
      try {
        setResult(null)
        setPlan(await api.prepare(request))
      } catch (cause) {
        fail(cause)
      }
    },
    execute: async () => {
      if (!plan) return
      const id = crypto.randomUUID()
      setJob(id)
      setError(null)
      try {
        setResult(await api.execute(plan.id, id))
        setData(await api.snapshot())
      } catch (cause) {
        fail(cause)
      } finally {
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
