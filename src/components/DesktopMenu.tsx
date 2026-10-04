import { useEffect, useEffectEvent, useState } from 'react'
import { toast } from 'sonner'
import { useStore } from '@/store'
import type { View } from '@/domain'
import { Button } from './ui/button'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog'

const views: View[] = ['overview', 'env', 'projects', 'worktrees', 'caches', 'activity', 'settings']
const shortcuts: Record<string, string> = {
  o: 'add-root',
  r: 'refresh',
  ',': 'settings',
  ...Object.fromEntries(views.slice(0, 6).map((view, index) => [String(index + 1), view])),
}

export function DesktopMenu() {
  const s = useStore()
  const { t } = s
  const [dialog, setDialog] = useState<'help' | 'about' | null>(null)
  const handle = useEffectEvent((action: string) => {
    if (action === 'help' || action === 'about') {
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
  useEffect(() => {
    if (!s.api.native || s.data.platform !== 'windows') return
    // WebView2 can consume accelerators before the native menu receives them.
    // Handle only keys delivered to the webview; native-handled keys never arrive here.
    const keydown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || event.altKey || event.shiftKey) return
      let action: string | undefined
      if (event.key === 'F1' && !event.ctrlKey && !event.metaKey) action = 'help'
      else if (event.ctrlKey && !event.metaKey) {
        action = shortcuts[event.key.toLowerCase()]
      }
      if (!action) return
      event.preventDefault()
      if (!event.repeat) handle(action)
    }
    window.addEventListener('keydown', keydown)
    return () => window.removeEventListener('keydown', keydown)
  }, [s.api.native, s.data.platform])
  const modifier = s.data.platform === 'macos' ? '⌘' : 'Ctrl'
  return (
    <Dialog
      open={dialog !== null}
      onOpenChange={(open) => {
        if (!open) setDialog(null)
      }}
    >
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {dialog === 'about'
              ? t('关于 Envark', 'About Envark')
              : t('使用说明与快捷键', 'Getting started & shortcuts')}
          </DialogTitle>
          <DialogDescription>
            {dialog === 'about'
              ? `Envark ${s.data.version}`
              : t(
                  '从扫描开始，执行前审阅每项变更。',
                  'Start with a scan and review each change before executing it.',
                )}
          </DialogDescription>
        </DialogHeader>
        {dialog === 'about' ? (
          <div className="space-y-3 text-sm">
            <p>
              {t(
                '集中查看和管理开发环境、工具、项目产物与全局缓存。',
                'Inspect and manage developer environments, tools, project artifacts, and shared caches.',
              )}
            </p>
            <p className="select-text break-all font-mono text-xs text-muted-foreground">
              https://github.com/felixpy/envark
            </p>
          </div>
        ) : (
          <div className="space-y-4 text-sm">
            <ol className="list-decimal space-y-2 pl-5">
              <li>
                {t(
                  '添加存放 Git 仓库的目录。关联 Worktree 会自动纳入扫描。',
                  'Add a folder containing Git repositories. Linked worktrees are included automatically.',
                )}
              </li>
              <li>
                {t(
                  '在项目或缓存页选择可操作项，点击审阅清理，确认后执行。',
                  'Select eligible items in Projects or Caches, review the cleanup, and confirm to execute.',
                )}
              </li>
              <li>
                {t(
                  '不可用选项会显示原因；检查更新需要先在设置中开启联网检查。',
                  'Unavailable actions explain why. Enable online update checks in Settings to check tool versions.',
                )}
              </li>
            </ol>
            <dl className="grid grid-cols-[1fr_auto] gap-2 rounded-lg bg-muted p-3 text-xs">
              <dt>{t('添加目录', 'Add folder')}</dt>
              <dd>{modifier}+O</dd>
              <dt>{t('重新扫描', 'Rescan')}</dt>
              <dd>{modifier}+R</dd>
              <dt>{t('设置', 'Settings')}</dt>
              <dd>{modifier}+,</dd>
              <dt>{t('切换主要页面', 'Switch main pages')}</dt>
              <dd>{modifier}+1–6</dd>
              <dt>{t('帮助', 'Help')}</dt>
              <dd>F1</dd>
            </dl>
            <Button
              variant="outline"
              onClick={() => {
                setDialog(null)
                s.go('settings')
              }}
            >
              {t('打开设置', 'Open settings')}
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  )
}
