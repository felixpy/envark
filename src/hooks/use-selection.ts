import { useState } from 'react'

export function useSelection(eligible: string[]) {
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const chosen = eligible.filter((id) => selected.has(id))
  const toggle = (ids: string[], on: boolean) =>
    setSelected((current) => {
      const next = new Set(current)
      for (const id of ids) {
        if (on) next.add(id)
        else next.delete(id)
      }
      return next
    })
  return {
    eligibleCount: eligible.length,
    selected,
    chosen,
    checked: chosen.length > 0 && chosen.length === eligible.length,
    toggle,
    toggleAll: (on: boolean) => toggle(eligible, on),
  }
}
