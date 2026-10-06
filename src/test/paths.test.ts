import { describe, expect, it } from 'vitest'
import { displayPath } from '@/lib/paths'

describe('path presentation', () => {
  it('formats drive and UNC paths without changing ordinary or device paths', () => {
    expect(displayPath(String.raw`\\?\C:\repos\app`)).toBe(String.raw`C:\repos\app`)
    expect(displayPath(String.raw`\\?\UNC\server\share\app`)).toBe(String.raw`\\server\share\app`)
    for (const path of [
      '/repos/app',
      String.raw`C:\repos\app`,
      String.raw`\\server\share`,
      String.raw`\\?\Volume{123}\app`,
    ]) {
      expect(displayPath(path)).toBe(path)
    }
  })
})
