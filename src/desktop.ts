import type { Settings } from './domain'

export const appLinks = {
  github: 'https://github.com/felixpy/envark',
  issues: 'https://github.com/felixpy/envark/issues/new/choose',
  releases: 'https://github.com/felixpy/envark/releases',
  latest: 'https://github.com/felixpy/envark/releases/latest',
} as const
export type AppLinkTarget = keyof typeof appLinks
export interface AppRelease {
  version: string
  available: boolean
}
export interface ViewState {
  sidebar: boolean
  zoom: number
  theme: Settings['theme']
}
export const zoomLevels = [0.8, 0.9, 1, 1.1, 1.25, 1.5] as const

export function nextZoom(current: number, action: string): number {
  if (action === 'zoom-reset') return 1
  if (action === 'zoom-in') return zoomLevels.find((value) => value > current) ?? zoomLevels.at(-1)!
  return [...zoomLevels].reverse().find((value) => value < current) ?? zoomLevels[0]
}
