import { expect, it } from 'vitest'
import { shortcutForEvent } from '@/shortcuts'

it('uses the platform modifier and ignores composition or unrelated key combinations', () => {
  const key = (init: KeyboardEventInit, platform = 'windows') =>
    shortcutForEvent(new KeyboardEvent('keydown', init), platform)
  expect(key({ key: 'b', ctrlKey: true })).toBe('toggle-sidebar')
  expect(key({ key: 'b', metaKey: true }, 'macos')).toBe('toggle-sidebar')
  expect(key({ key: 'b', ctrlKey: true }, 'macos')).toBeUndefined()
  expect(key({ key: 'b', metaKey: true })).toBeUndefined()
  expect(key({ key: '=', ctrlKey: true })).toBe('zoom-in')
  expect(key({ key: '+', ctrlKey: true, shiftKey: true })).toBe('zoom-in')
  expect(key({ key: 'r', ctrlKey: true, shiftKey: true })).toBeUndefined()
  expect(key({ key: 'r', ctrlKey: true, altKey: true })).toBeUndefined()
  expect(key({ key: 'r', ctrlKey: true, isComposing: true })).toBeUndefined()
  expect(key({ key: 'F1', shiftKey: true })).toBeUndefined()
  expect(key({ key: 'F1', ctrlKey: true })).toBeUndefined()
})
