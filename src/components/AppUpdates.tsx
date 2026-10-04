import { createContext, useContext, useRef, useState, type ReactNode } from 'react'
import { CheckCircle2, CircleAlert, Download, RefreshCw } from 'lucide-react'
import type { AppRelease } from '@/desktop'
import { useStore } from '@/store'
import { AppLink } from './AppLink'
import { Button } from './ui/button'
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog'

const Context = createContext<(() => void) | null>(null)
export function useAppUpdates() {
  const check = useContext(Context)
  if (!check) throw new Error('App update provider is missing')
  return check
}

type CheckState =
  | { status: 'checking' }
  | { status: 'done'; release: AppRelease }
  | { status: 'error'; message: string }

export function AppUpdateProvider({ children }: { children: ReactNode }) {
  const { api, data, t } = useStore()
  const [open, setOpen] = useState(false)
  const [state, setState] = useState<CheckState>({ status: 'checking' })
  const checking = useRef(false)
  const check = async () => {
    setOpen(true)
    if (checking.current) return
    checking.current = true
    setState({ status: 'checking' })
    try {
      if (!api.checkAppUpdate)
        throw new Error(
          t('请在桌面客户端中检查更新。', 'Open the desktop app to check for updates.'),
        )
      setState({ status: 'done', release: await api.checkAppUpdate() })
    } catch (error) {
      setState({ status: 'error', message: String(error) })
    } finally {
      checking.current = false
    }
  }
  return (
    <Context.Provider value={() => void check()}>
      {children}
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('检查 Envark 更新', 'Check for Envark updates')}</DialogTitle>
            <DialogDescription>
              {t('当前版本', 'Current version')} · {data.version}
            </DialogDescription>
          </DialogHeader>
          <div className="space-y-3 py-2 text-sm" role="status">
            {state.status === 'checking' && (
              <p className="flex items-center gap-2">
                <RefreshCw className="size-4 animate-spin" />
                {t('正在检查最新发布版本…', 'Checking the latest release…')}
              </p>
            )}
            {state.status === 'done' && (
              <>
                <p className="flex items-center gap-2 font-medium">
                  {state.release.available ? (
                    <Download className="size-4" />
                  ) : (
                    <CheckCircle2 className="size-4" />
                  )}
                  {state.release.available
                    ? `${t('发现新版本', 'New version available')} · ${state.release.version}`
                    : t('当前已是最新版本。', 'You are up to date.')}
                </p>
                {state.release.available && (
                  <p className="text-muted-foreground">
                    {t(
                      '打开发布页面，下载适用于当前系统的安装包。',
                      'Open the release page to download the installer for your system.',
                    )}
                  </p>
                )}
              </>
            )}
            {state.status === 'error' && (
              <>
                <p className="flex items-center gap-2">
                  <CircleAlert className="size-4" />
                  {t(
                    '暂时无法检查更新，请重试或查看发布页面。',
                    'Unable to check for updates. Retry or visit the release page.',
                  )}
                </p>
                <p className="break-all text-xs text-muted-foreground">{state.message}</p>
              </>
            )}
          </div>
          <DialogFooter>
            <Button variant="outline" onClick={() => setOpen(false)}>
              {t('关闭', 'Close')}
            </Button>
            {state.status === 'error' && (
              <Button variant="outline" onClick={() => void check()}>
                {t('重试', 'Retry')}
              </Button>
            )}
            {state.status !== 'checking' && (
              <Button asChild>
                <AppLink target="latest">{t('查看发布页面', 'View release page')}</AppLink>
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Context.Provider>
  )
}
