import type { ComponentProps } from 'react'
import { toast } from 'sonner'
import { appLinks, type AppLinkTarget } from '@/desktop'
import { useStore } from '@/store'

export function AppLink({
  target,
  children,
  ...props
}: Omit<ComponentProps<'a'>, 'href' | 'target' | 'onClick'> & { target: AppLinkTarget }) {
  const { api, t } = useStore()
  return (
    <a
      {...props}
      href={appLinks[target]}
      target="_blank"
      rel="noopener noreferrer"
      onClick={(event) => {
        if (!api.native) return
        event.preventDefault()
        void (
          api.openAppLink?.(target) ??
          Promise.reject(new Error('Browser integration is unavailable.'))
        ).catch((error: unknown) =>
          toast.error(t('无法打开浏览器', 'Unable to open browser'), {
            description: String(error),
          }),
        )
      }}
    >
      {children}
    </a>
  )
}
