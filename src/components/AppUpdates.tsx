import { createContext, useContext, useRef, useState, type ReactNode } from 'react'
import { CheckCircle2, CircleAlert, Download, RefreshCw } from 'lucide-react'
import type { AppRelease, AppUpdateProgress } from '@/desktop'
import { formatBytes } from '@/domain'
import { useStore } from '@/store'
import { mergeAppUpdateProgress } from '@/lib/app-update-progress'
import { AppLink } from './AppLink'
import { Button } from './ui/button'
import { ActionButton } from './action-controls'
import { Progress } from './ui/progress'
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
  | { status: 'error'; message: string; phase?: 'install' }
  | { status: 'downloading'; progress: AppUpdateProgress }
  | { status: 'installed'; message?: string }

export function AppUpdateProvider({ children }: { children: ReactNode }) {
  const { api, data, t, busy, plan } = useStore()
  const [open, setOpen] = useState(false)
  const [state, setState] = useState<CheckState>({ status: 'checking' })
  const checking = useRef(false)
  const installing = useRef(false)
  const restartReady = useRef(false)
  const busyReason =
    busy || plan
      ? t(
          '请先完成当前操作或关闭审阅窗口。',
          'Finish the current operation or close the review dialog first.',
        )
      : null
  const check = async () => {
    setOpen(true)
    if (checking.current || installing.current || restartReady.current) return
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
  const install = async () => {
    if (installing.current || busy || plan) return
    installing.current = true
    setState({ status: 'downloading', progress: { downloaded: 0, total: null, installing: false } })
    let unsubscribe: (() => void) | undefined
    let acceptingProgress = true
    try {
      if (!api.installAppUpdate || !api.subscribeAppUpdate)
        throw new Error(
          t('此客户端不支持自动更新。', 'Automatic updates are unavailable in this client.'),
        )
      unsubscribe = await api.subscribeAppUpdate((progress) => {
        if (!acceptingProgress) return
        setState((previous) =>
          previous.status === 'downloading'
            ? {
                status: 'downloading',
                progress: mergeAppUpdateProgress(previous.progress, progress),
              }
            : previous,
        )
      })
      await api.installAppUpdate()
      restartReady.current = true
      setState({ status: 'installed' })
    } catch (error) {
      setState({ status: 'error', message: String(error), phase: 'install' })
    } finally {
      acceptingProgress = false
      unsubscribe?.()
      installing.current = false
    }
  }
  const restart = async () => {
    if (busy || plan) return
    try {
      if (!api.restartAfterUpdate)
        throw new Error(
          t('请退出后重新打开 Envark。', 'Quit and reopen Envark to finish updating.'),
        )
      await api.restartAfterUpdate()
    } catch (error) {
      setState({ status: 'installed', message: String(error) })
    }
  }
  return (
    <Context.Provider value={() => void check()}>
      {children}
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!installing.current) setOpen(next)
        }}
      >
        <DialogContent showCloseButton={state.status !== 'downloading'}>
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
                      '下载并安装更新，完成后重新启动 Envark。',
                      'Download and install the update, then restart Envark.',
                    )}
                  </p>
                )}
                {state.release.available && !state.release.installable && (
                  <p
                    className="text-muted-foreground"
                    title={state.release.installIssue ?? undefined}
                  >
                    {t(
                      '此版本暂不支持自动安装，请从发布页面下载安装包。',
                      'Automatic installation is unavailable for this release. Download its installer from the release page.',
                    )}
                  </p>
                )}
              </>
            )}
            {state.status === 'downloading' && (
              <div className="space-y-3">
                <p className="flex items-center gap-2">
                  <RefreshCw aria-hidden="true" className="size-4 animate-spin" />
                  {state.progress.installing
                    ? t('正在安装更新…', 'Installing update…')
                    : t('正在下载更新…', 'Downloading update…')}
                </p>
                {(state.progress.total || state.progress.installing) && (
                  <Progress
                    aria-label={t('正在下载更新…', 'Downloading update…')}
                    value={
                      state.progress.installing
                        ? 100
                        : Math.min(100, (state.progress.downloaded / state.progress.total!) * 100)
                    }
                    className="[&>[data-slot=progress-indicator]]:transition-transform [&>[data-slot=progress-indicator]]:duration-100 [&>[data-slot=progress-indicator]]:ease-linear [&>[data-slot=progress-indicator]]:motion-reduce:transition-none"
                  />
                )}
                <p className="flex justify-between font-mono text-xs text-muted-foreground">
                  <span>
                    {formatBytes(state.progress.downloaded)}
                    {state.progress.total ? ` / ${formatBytes(state.progress.total)}` : ''}
                  </span>
                  {state.progress.total && (
                    <span>
                      {Math.min(
                        100,
                        Math.floor((state.progress.downloaded / state.progress.total) * 100),
                      )}
                      %
                    </span>
                  )}
                </p>
              </div>
            )}
            {state.status === 'installed' && (
              <>
                <p className="flex items-center gap-2">
                  <CheckCircle2 className="size-4" />
                  {t('更新已安装，重新启动后生效。', 'Update installed. Restart Envark to finish.')}
                </p>
                {state.message && (
                  <p className="break-all text-xs text-muted-foreground">{state.message}</p>
                )}
              </>
            )}
            {state.status === 'error' && (
              <>
                <p className="flex items-center gap-2">
                  <CircleAlert className="size-4" />
                  {state.phase === 'install'
                    ? t(
                        '更新安装失败，请重试或查看发布页面。',
                        'Unable to install the update. Retry or visit the release page.',
                      )
                    : t(
                        '暂时无法检查更新，请重试或查看发布页面。',
                        'Unable to check for updates. Retry or visit the release page.',
                      )}
                </p>
                <p className="break-all text-xs text-muted-foreground">{state.message}</p>
              </>
            )}
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              disabled={state.status === 'downloading'}
              onClick={() => setOpen(false)}
            >
              {t('关闭', 'Close')}
            </Button>
            {state.status === 'error' && (
              <Button variant="outline" onClick={() => void check()}>
                {t('重试', 'Retry')}
              </Button>
            )}
            {state.status === 'done' && state.release.available && state.release.installable && (
              <ActionButton reason={busyReason} onClick={() => void install()}>
                <Download />
                {t('下载并安装', 'Download and install')}
              </ActionButton>
            )}
            {state.status === 'installed' && (
              <ActionButton reason={busyReason} onClick={() => void restart()}>
                {t('立即重启', 'Restart now')}
              </ActionButton>
            )}
            {(state.status === 'done' || state.status === 'error') && (
              <Button asChild variant="outline">
                <AppLink target="latest">{t('查看发布页面', 'View release page')}</AppLink>
              </Button>
            )}
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Context.Provider>
  )
}
