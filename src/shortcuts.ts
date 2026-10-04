export const shortcutGroups = [
  {
    id: 'file',
    label: ['文件', 'File'],
    shortcuts: [
      { id: 'add-root', label: ['添加目录', 'Add folder'], key: 'O' },
      { id: 'refresh', label: ['重新扫描', 'Rescan'], key: 'R' },
      { id: 'settings', label: ['设置', 'Settings'], key: ',' },
    ],
  },
  {
    id: 'navigation',
    label: ['页面导航', 'Navigation'],
    shortcuts: [
      { id: 'overview', label: ['概览', 'Overview'], key: '1' },
      { id: 'env', label: ['环境与工具', 'Environments & tools'], key: '2' },
      { id: 'projects', label: ['项目空间', 'Project space'], key: '3' },
      { id: 'caches', label: ['全局缓存', 'Global caches'], key: '4' },
      { id: 'activity', label: ['操作记录', 'Activity'], key: '5' },
    ],
  },
  {
    id: 'view',
    label: ['视图', 'View'],
    shortcuts: [
      { id: 'toggle-sidebar', label: ['切换侧边栏', 'Toggle sidebar'], key: 'B' },
      { id: 'zoom-in', label: ['放大', 'Zoom in'], key: '+' },
      { id: 'zoom-out', label: ['缩小', 'Zoom out'], key: '-' },
      { id: 'zoom-reset', label: ['实际大小 (100%)', 'Actual size (100%)'], key: '0' },
    ],
  },
  {
    id: 'help',
    label: ['帮助', 'Help'],
    shortcuts: [{ id: 'shortcuts', label: ['键盘快捷键', 'Keyboard shortcuts'], key: 'F1' }],
  },
] as const

type Shortcut = (typeof shortcutGroups)[number]['shortcuts'][number]
export type ShortcutId = Shortcut['id']
const shortcuts: readonly Shortcut[] = shortcutGroups.flatMap((group) => [...group.shortcuts])

export function shortcutForEvent(event: KeyboardEvent, platform: string): ShortcutId | undefined {
  if (event.altKey || event.isComposing) return
  const modified =
    platform === 'macos' ? event.metaKey && !event.ctrlKey : event.ctrlKey && !event.metaKey
  if (!event.ctrlKey && !event.metaKey && !event.shiftKey && event.key === 'F1') return 'shortcuts'
  if (!modified) return
  if (event.key === '+' || event.key === '=') return 'zoom-in'
  if (event.shiftKey) return
  return shortcuts.find(
    (shortcut) => shortcut.key !== 'F1' && shortcut.key.toLowerCase() === event.key.toLowerCase(),
  )?.id
}

export function shortcutLabel(key: string, platform: string): string {
  if (key === 'F1') return key
  return platform === 'macos' ? `⌘ ${key}` : `Ctrl + ${key}`
}
