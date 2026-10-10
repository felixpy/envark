import { useCallback, useEffect, useRef, useState, type Dispatch, type SetStateAction } from 'react'
import type { Backend } from './bridge'
import type { Provider, ProviderId, Snapshot, Tool } from './domain'

export type UpdateCheck = 'checking' | 'checked' | 'error'
const tools = (provider: Provider) => [...provider.packageManagers, ...provider.tools]
const fingerprint = (provider: Provider) =>
  JSON.stringify(tools(provider).map(({ latest: _latest, updateStatus: _status, ...tool }) => tool))

// Only apply results to installations that have not changed since the lookup began.
export function mergeToolUpdates(current: Provider, before: Provider, updated: Provider): Provider {
  const merge = (items: Tool[], originals: Tool[], results: Tool[]) =>
    items.map((item) => {
      const original = originals.find((tool) => tool.id === item.id)
      const result = results.find((tool) => tool.id === item.id)
      return original && result && JSON.stringify(item) === JSON.stringify(original) ? result : item
    })
  return {
    ...current,
    packageManagers: merge(
      current.packageManagers,
      before.packageManagers,
      updated.packageManagers,
    ),
    tools: merge(current.tools, before.tools, updated.tools),
  }
}

export function useToolUpdates(
  api: Backend,
  data: Snapshot,
  setData: Dispatch<SetStateAction<Snapshot>>,
  activeProvider: ProviderId | null,
) {
  const [checks, setChecks] = useState<Partial<Record<ProviderId, UpdateCheck>>>({})
  const current = useRef(data)
  current.current = data
  const alive = useRef(true)
  const pending = useRef(new Map<ProviderId, string>())
  const cache = useRef(new Map<ProviderId, { key: string; expires: number }>())
  const check = useCallback(
    async (id: ProviderId, force = true) => {
      const snapshot = current.current
      const before = snapshot.inventory.providers.find((item) => item.id === id)
      if (!alive.current || !snapshot.settings.checkUpdates || !before || pending.current.has(id))
        return
      const key = fingerprint(before)
      const cached = cache.current.get(id)
      if (!force && (!tools(before).length || (cached?.key === key && cached.expires > Date.now())))
        return
      const job = crypto.randomUUID()
      pending.current.set(id, job)
      setChecks((states) => ({ ...states, [id]: 'checking' }))
      try {
        const updated = await api.checkToolUpdates(id, job)
        if (!alive.current || pending.current.get(id) !== job) return
        const incomplete = tools(updated).some(
          (tool) => tool.canUpdate && tool.updateStatus === 'unknown',
        )
        cache.current.set(id, { key, expires: Date.now() + (incomplete ? 60000 : 15 * 60000) })
        setData((state) =>
          state.settings.checkUpdates
            ? {
                ...state,
                inventory: {
                  ...state.inventory,
                  providers: state.inventory.providers.map((item) =>
                    item.id === id ? mergeToolUpdates(item, before, updated) : item,
                  ),
                },
              }
            : state,
        )
        setChecks((states) => ({ ...states, [id]: incomplete ? 'error' : 'checked' }))
      } catch {
        if (!alive.current || pending.current.get(id) !== job) return
        cache.current.set(id, { key, expires: Date.now() + 60000 })
        setChecks((states) => ({ ...states, [id]: 'error' }))
      } finally {
        if (pending.current.get(id) === job) pending.current.delete(id)
      }
    },
    [api, setData],
  )
  useEffect(() => {
    alive.current = true
    const jobs = pending.current
    return () => {
      alive.current = false
      for (const job of jobs.values()) void api.cancel(job).catch(() => {})
      jobs.clear()
    }
  }, [api])
  useEffect(() => {
    if (!data.settings.checkUpdates) {
      for (const job of pending.current.values()) void api.cancel(job).catch(() => {})
      pending.current.clear()
      cache.current.clear()
      setChecks({})
    }
  }, [api, data.settings.checkUpdates])
  useEffect(() => {
    if (!api.native || !activeProvider || !data.settings.checkUpdates) return
    const ensure = () => {
      if (!document.hidden) void check(activeProvider, false)
    }
    ensure()
    const timer = window.setInterval(ensure, 60000)
    window.addEventListener('focus', ensure)
    document.addEventListener('visibilitychange', ensure)
    return () => {
      window.clearInterval(timer)
      window.removeEventListener('focus', ensure)
      document.removeEventListener('visibilitychange', ensure)
    }
  }, [api.native, activeProvider, data, checks, check])
  return { updateChecks: checks, checkToolUpdates: check }
}
