import { describe, expect, it } from 'vitest'
import { idle, updateKind, type Project, type Tool } from '@/domain'

describe('uncertain inventory data', () => {
  it('never treats missing or incomplete activity as evidence of inactivity', () => {
    expect(idle({ lastActive: null, activityComplete: true } as Project, 90)).toBe(false)
    expect(idle({ lastActive: 1, activityComplete: false } as Project, 90)).toBe(false)
  })
  it('does not report a tool as current before a successful version check', () => {
    expect(updateKind({ version: '2.0.0', latest: null } as Tool)).toBe('unknown')
    expect(updateKind({ version: '2.0.0', latest: '3.0.0' } as Tool)).toBe('major')
  })
})
