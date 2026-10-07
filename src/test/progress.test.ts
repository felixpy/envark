import { expect, it } from 'vitest'
import { progressLabel } from '@/lib/progress'
import type { Progress } from '@/domain'

const event = (stage: string): Progress => ({
  stage,
  jobId: 'scan',
  completed: 3,
  total: null,
  message: 'Backend diagnostic',
})
const english = (_zh: string, en: string) => en
const chinese = (zh: string) => zh

it('localizes scan phases without exposing backend diagnostic text', () => {
  expect(progressLabel(event('environments'), english)).toBe('Detecting development environments')
  expect(progressLabel(event('measure-projects'), english)).toBe('Measuring project artifacts')
  expect(progressLabel(event('measure-worktrees'), chinese)).toBe('正在计算工作树占用')
  expect(progressLabel(event('projects-complete'), english)).toBe(
    'Project scan complete · 3 projects',
  )
  expect(progressLabel(event('discover'), english)).toBe('Scanning projects · 3 paths checked')
})

it('preserves the operation title and distinguishes execution before its first event', () => {
  expect(progressLabel(null, english, true)).toBe('Executing operations')
  expect(progressLabel(null, english)).toBe('Scanning')
  expect(
    progressLabel({ ...event('execute'), total: 5, message: 'Clean npm cache' }, english),
  ).toBe('Executing operations · 3/5 · Clean npm cache')
})
