import { createContext, useContext, useEffect, useRef, useState, type ReactNode } from 'react'
import { toast } from 'sonner'
import { useToolUpdates, type UpdateCheck } from './tool-updates'
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
  task: {
    kind: 'scan' | 'updates' | 'prepare' | 'execute'
    startedAt: number
    request: ActionRequest | null
  } | null
  progress: Progress | null
  cancel(): Promise<void>
  refresh(): Promise<void>
  updateChecks: Partial<Record<ProviderId, UpdateCheck>>
  checkToolUpdates(provider: ProviderId): Promise<void>
  saveSettings(settings: Settings): Promise<boolean>
  setTheme(theme: Settings['theme']): Promise<void>
  setDisabledShortcuts(disabled: Settings['disabledShortcuts']): Promise<void>
  addRoot(path?: string): Promise<void>
  plan: Plan | null
  operationOpen: boolean
  result: OperationResult | null
  operationError: string | null
  completedSelectionIds: string[]
  prepare(request: ActionRequest): Promise<void>
  reviewAgain(): Promise<void>
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
  const jobRef = useRef<string | null>(null)
  const [task, setTask] = useState<Store['task']>(null)
  const [progress, setProgress] = useState<Progress | null>(null)
  const [plan, setPlan] = useState<Plan | null>(null)
  const [result, setResult] = useState<OperationResult | null>(null)
  const [operationError, setOperationError] = useState<string | null>(null)
  const [completedSelectionIds, setCompletedSelectionIds] = useState<string[]>([])
  const launched = useRef(false)
  const running = useRef(false)
  const reviewedRequest = useRef<ActionRequest | null>(null)
  const cancelledPreparation = useRef<string | null>(null)
  const begin = (
    kind: NonNullable<Store['task']>['kind'],
    request: ActionRequest | null = null,
  ) => {
    const id = crypto.randomUUID()
    running.current = true
    jobRef.current = id
    setJob(id)
    setTask({ kind, startedAt: Date.now(), request })
    setProgress(null)
    setError(null)
    return id
  }
  const finish = () => {
    running.current = false
    jobRef.current = null
    setJob(null)
    setTask(null)
    setProgress(null)
  }
  const t = (zh: string, en: string) =>
    data.settings.language === 'en' ? en : data.settings.language === 'zh-TW' ? traditional(zh) : zh
  const fail = (cause: unknown) => {
    const message = String(cause)
    if (message.includes('The operation was cancelled.')) {
      setError(null)
      return
    }
    setError(message)
    toast.error(message)
  }
  const refresh = async () => {
    if (running.current) return
    const id = begin('scan')
    try {
      setData(await api.refresh(id))
    } catch (cause) {
      fail(cause)
    } finally {
      finish()
    }
  }
  useEffect(() => {
    let active = true
    let unsubscribe: (() => void) | undefined
    void api
      .subscribe((event) => {
        if (active && event.jobId === jobRef.current) setProgress(event)
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
    if (!loaded || !api.refreshDisks) return
    let active = true
    let pending = false
    let updated = 0
    const refreshDisks = async () => {
      if (pending || document.hidden || Date.now() - updated < 5000) return
      pending = true
      try {
        const disks = await api.refreshDisks!()
        if (active)
          setData((current) => ({ ...current, inventory: { ...current.inventory, disks } }))
        updated = Date.now()
      } catch {
        // Keep the previous reading when a transient OS query fails.
      } finally {
        pending = false
      }
    }
    void refreshDisks()
    const timer = window.setInterval(() => void refreshDisks(), 30000)
    window.addEventListener('focus', refreshDisks)
    document.addEventListener('visibilitychange', refreshDisks)
    return () => {
      active = false
      window.clearInterval(timer)
      window.removeEventListener('focus', refreshDisks)
      document.removeEventListener('visibilitychange', refreshDisks)
    }
  }, [api, loaded])
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
  const updates = useToolUpdates(api, data, setData, loaded && view === 'env' ? provider : null)
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
    task,
    progress,
    plan,
    operationOpen:
      !!plan || !!operationError || task?.kind === 'prepare' || task?.kind === 'execute',
    result,
    operationError,
    completedSelectionIds,
    t,
    go: (next, id, nextFocus = {}) => {
      setView(next)
      setProvider(id ?? null)
      setFocus(nextFocus)
      setNavigationKey((key) => key + 1)
    },
    refresh,
    ...updates,
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
      if (!job) return
      if (task?.kind === 'prepare') cancelledPreparation.current = job
      await api.cancel(job)
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
      const id = begin('prepare', request)
      cancelledPreparation.current = null
      reviewedRequest.current = request
      setPlan(null)
      setResult(null)
      setOperationError(null)
      try {
        const prepared = await api.prepare(request, id)
        if (cancelledPreparation.current !== id) setPlan(prepared)
      } catch (cause) {
        if (
          cancelledPreparation.current !== id &&
          !String(cause).includes('The operation was cancelled.')
        ) {
          setOperationError(String(cause))
        }
      } finally {
        finish()
      }
    },
    reviewAgain: async () => {
      if (reviewedRequest.current) await store.prepare(reviewedRequest.current)
    },
    execute: async (discardWorktreeChanges = false) => {
      if (!plan || result || operationError || running.current) return
      const id = begin('execute', reviewedRequest.current)
      try {
        const outcome = await api.execute(plan.id, id, discardWorktreeChanges)
        setResult(outcome)
        // Paths can be shared by distinct models or tools. Only the backend's
        // IDs for the step that actually succeeded can clear a selection.
        setCompletedSelectionIds(
          outcome.items.flatMap((item) =>
            item.status === 'success' ? (item.targetIds ?? []) : [],
          ),
        )
        try {
          setData(await api.snapshot())
        } catch (cause) {
          // The operation already completed. Keep its results and never offer
          // to execute the consumed plan again when only the refresh failed.
          fail(cause)
        }
      } catch (cause) {
        setOperationError(String(cause))
      } finally {
        finish()
      }
    },
    closePlan: () => {
      if (!job) {
        setPlan(null)
        setResult(null)
        setOperationError(null)
      }
    },
    reload: async () => {
      setData(await api.snapshot())
    },
  }
  return <Context.Provider value={store}>{children}</Context.Provider>
}
