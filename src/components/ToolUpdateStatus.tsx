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
  return null
}
