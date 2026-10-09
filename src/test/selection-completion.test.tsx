import { act, renderHook } from '@testing-library/react'
import { expect, it } from 'vitest'
import { useSelection } from '@/hooks/use-selection'

it('clears completed selections across removal and regeneration while retaining failures for retry', () => {
  const { result, rerender } = renderHook(
    ({ eligible, completed }) => useSelection(eligible, completed),
    { initialProps: { eligible: ['success', 'failure'], completed: [] as string[] } },
  )
  act(() => result.current.toggleAll(true))
  const completed = ['success']
  rerender({ eligible: ['failure'], completed })
  expect(result.current.chosen).toEqual(['failure'])
  expect(result.current.selected.has('success')).toBe(false)
  // A new build restores the same artifact ID, without another user selection.
  rerender({ eligible: ['success', 'failure'], completed })
  expect(result.current.chosen).toEqual(['failure'])
  // Clearing a completed operation does not prevent a deliberate future selection.
  act(() => result.current.toggle(['success'], true))
  expect(result.current.chosen).toEqual(['success', 'failure'])
})
