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
          '部分版本查询失败，原因已显示在对应工具下方。将自动重试，也可立即重新检查。',
          'Some version lookups failed; see the reason below each tool. Retrying automatically, or check again now.',
        )}
      </p>
    )
  return null
}
