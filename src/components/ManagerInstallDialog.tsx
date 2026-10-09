import { useState } from 'react'
import { Plus } from 'lucide-react'
import { useStore } from '@/store'
import type { ManagerInstallOption, ProviderId } from '@/domain'
import { ActionButton as Button } from './action-controls'
import { Badge } from './ui/badge'
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from './ui/dialog'

export function ManagerInstallDialog({
  provider,
  prominent = false,
}: {
  provider: ProviderId
  prominent?: boolean
}) {
  const s = useStore()
  const { t } = s
  const title =
    provider === 'ollama' ? t('安装 Ollama', 'Install Ollama') : t('安装管理器', 'Install manager')
  const [open, setOpen] = useState(false)
  const [options, setOptions] = useState<ManagerInstallOption[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState('')
  const load = async () => {
    setOpen(true)
    setLoading(true)
    setError('')
    try {
      setOptions(await s.api.managerOptions(provider))
    } catch (error) {
      setError(String(error))
    } finally {
      setLoading(false)
    }
  }
  return (
    <>
      <Button
        variant={prominent ? 'default' : 'outline'}
        size="sm"
        reason={
          s.busy ? t('请等待当前操作完成。', 'Wait for the current operation to finish.') : null
        }
        onClick={() => void load()}
      >
        <Plus />
        {title}
      </Button>
      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-h-[85dvh] overflow-y-auto sm:max-w-xl">
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
            <DialogDescription>
              {provider === 'ollama'
                ? t(
                    '审阅程序版本、安装来源和路径后执行，已有模型和配置会保留。',
                    'Review the application version, source, and destination. Existing models and configuration are kept.',
                  )
                : t(
                    '选择管理器，查看版本、安装来源和路径后执行。已有运行时和项目会保留。',
                    'Choose a manager, then review its version, source, and installation path. Existing runtimes and projects are kept.',
                  )}
            </DialogDescription>
          </DialogHeader>
          {loading ? (
            <p role="status">{t('正在检查安装条件…', 'Checking installation prerequisites…')}</p>
          ) : error ? (
            <div role="alert" className="space-y-2">
              <p>{error}</p>
              <Button variant="outline" onClick={() => void load()}>
                {t('重试', 'Retry')}
              </Button>
            </div>
          ) : (
            <div className="space-y-2">
              {options.map((option) => (
                <div
                  key={option.name}
                  className="flex items-center justify-between gap-4 rounded-lg border p-4"
                >
                  <div className="min-w-0 space-y-1">
                    <p className="font-medium">{option.name}</p>
                    <p className="text-xs text-muted-foreground">
                      {option.source === 'official-installer'
                        ? t('官方安装脚本', 'Official installer')
                        : option.source}
                    </p>
                    {!option.available && !option.installed && (
                      <p className="text-xs text-muted-foreground">{option.reason}</p>
                    )}
                  </div>
                  {option.installed ? (
                    <Badge variant="secondary">{t('已安装', 'Installed')}</Badge>
                  ) : (
                    <Button
                      size="sm"
                      variant="outline"
                      reason={
                        s.busy
                          ? t('请等待当前操作完成。', 'Wait for the current operation to finish.')
                          : !option.available
                            ? option.reason
                            : null
                      }
                      onClick={() => {
                        setOpen(false)
                        void s.prepare({ kind: 'installManager', provider, manager: option.name })
                      }}
                    >
                      {t('安装', 'Install')}
                    </Button>
                  )}
                </div>
              ))}
            </div>
          )}
        </DialogContent>
      </Dialog>
    </>
  )
}
