import { useEffect, useEffectEvent, useRef, useState } from 'react'
import { ExternalLink } from 'lucide-react'
import { toast } from 'sonner'
import { useStore } from '@/store'
import type { View } from '@/domain'
import { appLinks, nextZoom, type AppLinkTarget } from '@/desktop'
import { shortcutForEvent } from '@/shortcuts'
import { AppLink } from './AppLink'
import { useAppUpdates } from './AppUpdates'
import { ShortcutSettings } from './ShortcutSettings'
import { useSidebar } from './ui/sidebar'
import { Button } from './ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog'

const views: View[] = ['overview', 'env', 'projects', 'caches', 'activity', 'settings']

export function DesktopMenu() {
  const s = useStore()
  const { t } = s
  const sidebar = useSidebar()
  const checkAppUpdate = useAppUpdates()
  const [zoom, setZoom] = useState(1)
  const viewSync = useRef(Promise.resolve())
  const [dialog, setDialog] = useState<'shortcuts' | 'about' | null>(null)
  const showSidebar = sidebar.isMobile ? sidebar.openMobile : sidebar.open
  useEffect(() => {
    if (!s.loaded || !s.api.syncViewState) return
    const state = { sidebar: showSidebar, theme: s.data.settings.theme, zoom }
    // Keep rapid menu changes ordered across the native bridge.
    viewSync.current = viewSync.current
      .then(() => s.api.syncViewState!(state))
      .catch((error: unknown) => {
        toast.error(String(error))
      })
  }, [s.loaded, s.api, s.data.settings.theme, showSidebar, zoom])
  const handle = useEffectEvent((action: string) => {
    if (action === 'toggle-sidebar') {
      sidebar.toggleSidebar()
      return
    }
    if (['zoom-in', 'zoom-out', 'zoom-reset'].includes(action)) {
      setZoom((current) => nextZoom(current, action))
      return
    }
    if (action.startsWith('theme-')) {
      const theme = action.slice(6)
      if (theme === 'light' || theme === 'dark' || theme === 'system') void s.setTheme(theme)
      return
    }
    if (action === 'check-update') {
      setDialog(null)
      checkAppUpdate()
      return
    }
    if (Object.hasOwn(appLinks, action)) {
      const target = action as AppLinkTarget
      if (s.api.native)
        void s.api.openAppLink?.(target).catch((error: unknown) =>
          toast.error(t('无法打开浏览器', 'Unable to open browser'), {
            description: String(error),
          }),
        )
      else window.open(appLinks[target], '_blank', 'noopener,noreferrer')
      return
    }
    if (action === 'shortcuts' || action === 'about') {
      setDialog(action)
      return
    }
    if (views.includes(action as View)) {
      s.go(action as View)
      return
    }
    if (action !== 'refresh' && action !== 'add-root') return
    if (s.busy || s.plan) {
      toast.info(
        t(
          '请先完成当前操作或关闭审阅窗口。',
          'Finish the current operation or close the review dialog first.',
        ),
      )
      return
    }
    if (action === 'refresh') void s.refresh()
    else void s.addRoot()
  })
  useEffect(() => {
    let active = true
    let stop: (() => void) | undefined
    void s.api
      .subscribeMenu?.((action) => {
        if (active) handle(action)
      })
      .then((unsubscribe) => {
        if (active) stop = unsubscribe
        else unsubscribe()
      })
      .catch((error: unknown) => toast.error(String(error)))
    return () => {
      active = false
      stop?.()
    }
  }, [s.api])
  const handleShortcut = useEffectEvent((event: KeyboardEvent) => {
    if (event.defaultPrevented || !s.loaded) return
    const action = shortcutForEvent(event, s.data.platform)
    if (!action) return
    // Keep browser navigation intact in the development preview.
    if (!s.api.native && action !== 'toggle-sidebar' && action !== 'shortcuts') return
    // Suppress webview defaults even for disabled shortcuts (for example reload/zoom).
    event.preventDefault()
    if (!event.repeat && !s.data.settings.disabledShortcuts.includes(action)) handle(action)
  })
  useEffect(() => {
    // WebView2 can consume accelerators before the native menu receives them.
    // Handle only keys delivered to the webview; native-handled keys never arrive here.
    const keydown = (event: KeyboardEvent) => handleShortcut(event)
    window.addEventListener('keydown', keydown)
    return () => window.removeEventListener('keydown', keydown)
  }, [])
  return (
    <Dialog
      open={dialog !== null}
      onOpenChange={(open) => {
        if (!open) setDialog(null)
      }}
    >
      <DialogContent
        className={dialog === 'shortcuts' ? 'gap-0 p-0 sm:max-w-xl' : undefined}
        aria-describedby={dialog === 'about' ? 'about-description' : undefined}
      >
        <DialogHeader className={dialog === 'shortcuts' ? 'p-6' : undefined}>
          <DialogTitle>
            {dialog === 'about'
              ? t('关于 Envark', 'About Envark')
              : t('键盘快捷键', 'Keyboard shortcuts')}
          </DialogTitle>
          {dialog === 'about' && (
            <DialogDescription id="about-description">Envark {s.data.version}</DialogDescription>
          )}
        </DialogHeader>
        {dialog === 'about' ? (
          <div className="space-y-3 text-sm">
            <p>
              {t(
                '集中查看和管理开发环境、工具、项目产物与全局缓存。',
                'Inspect and manage developer environments, tools, project artifacts, and shared caches.',
              )}
            </p>
            <div className="flex flex-wrap items-center gap-4">
              <AppLink
                target="github"
                className="inline-flex items-center gap-2 underline underline-offset-4"
              >
                <ExternalLink className="size-4" />
                GitHub
              </AppLink>
              <Button
                variant="outline"
                size="sm"
                onClick={() => {
                  setDialog(null)
                  checkAppUpdate()
                }}
              >
                {t('检查更新', 'Check for updates')}
              </Button>
            </div>
          </div>
        ) : (
          <ShortcutSettings />
        )}
      </DialogContent>
    </Dialog>
  )
}
