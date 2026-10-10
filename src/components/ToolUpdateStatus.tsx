import { LoaderCircle } from 'lucide-react'
import type { ProviderId } from '@/domain'
import { useStore } from '@/store'

export function ToolUpdateStatus({ provider }: { provider: ProviderId }) {
  const { updateChecks, t } = useStore()
  const state = updateChecks[provider]
  if (state === 'checking')
    return (
      <p role="status" className="flex items-center gap-2 text-xs text-muted-foreground">
        <LoaderCircle className="size-3.5 animate-spin" />
        {t(
          '正在检查更新，可继续使用其他功能。',
          'Checking tool updates. You can keep using the app.',
        )}
      </p>
    )
  if (state === 'error')
    return (
      <p role="status" className="text-xs text-muted-foreground">
        {t(
          '部分版本信息暂不可用，将自动重试。也可点击“重新检查更新”立即重试。',
          'Some version information is unavailable. Retrying automatically, or check again now.',
        )}
      </p>
    )
  return null
}
