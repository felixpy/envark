import type { ReactNode } from 'react'
import { Loader2, Search } from 'lucide-react'
import { cn } from '@/lib/utils'
import { metadata, type ProviderId } from '@/domain'
import { Input } from './ui/input'

export function EcoDot({ id, className }: { id: ProviderId; className?: string }) {
  return (
    <span
      className={cn('inline-block size-2 shrink-0 rounded-full', className)}
      style={{ background: metadata[id].color }}
    />
  )
}
export function PageHeader({
  title,
  description,
  actions,
}: {
  title: string
  description?: string
  actions?: ReactNode
}) {
  return (
    <div className="flex flex-wrap items-end justify-between gap-4">
      <div className="space-y-1">
        <h1 className="text-2xl font-semibold tracking-tight">{title}</h1>
        {description && <p className="max-w-2xl text-sm text-muted-foreground">{description}</p>}
      </div>
      {actions && <div className="flex items-center gap-2">{actions}</div>}
    </div>
  )
}
export function Empty({ children }: { children: ReactNode }) {
  return (
    <div className="flex min-h-32 items-center justify-center p-6 text-center text-sm text-muted-foreground">
      {children}
    </div>
  )
}
export function SearchInput({
  value,
  onChange,
  placeholder,
}: {
  value: string
  onChange(value: string): void
  placeholder: string
}) {
  return (
    <div className="relative w-56">
      <Search className="absolute left-2.5 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
      <Input
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        aria-label={placeholder}
        className="pl-8"
      />
    </div>
  )
}
export function Spinner() {
  return <Loader2 className="size-4 animate-spin" />
}
