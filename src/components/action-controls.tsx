import type { ComponentProps, ReactNode } from 'react'
import { Button } from './ui/button'
import { Checkbox } from './ui/checkbox'

function Availability({ reason, children }: { reason?: string | null; children: ReactNode }) {
  return (
    <span
      className="inline-flex"
      title={reason || undefined}
      tabIndex={reason ? 0 : undefined}
      aria-label={reason || undefined}
    >
      {children}
    </span>
  )
}

export function ActionButton({
  reason,
  ...props
}: ComponentProps<typeof Button> & { reason?: string | null }) {
  return (
    <Availability reason={reason}>
      <Button
        {...props}
        disabled={Boolean(reason) || props.disabled}
        aria-description={reason || undefined}
      />
    </Availability>
  )
}

export function SelectionCheckbox({
  reason,
  ...props
}: ComponentProps<typeof Checkbox> & { reason?: string | null }) {
  return (
    <Availability reason={reason}>
      <Checkbox
        {...props}
        disabled={Boolean(reason) || props.disabled}
        aria-description={reason || undefined}
      />
    </Availability>
  )
}
