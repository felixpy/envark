import { useState, type ComponentProps, type ReactElement, type SyntheticEvent } from 'react'
import { cn } from 'cn'
import { Button } from './ui/button'
import { Checkbox } from './ui/checkbox'
import { Tooltip, TooltipContent, TooltipTrigger } from './ui/tooltip'

function Availability({ reason, children }: { reason?: string | null; children: ReactElement }) {
  const [open, setOpen] = useState(false)
  if (!reason) return children
  const explain = (event: SyntheticEvent) => {
    event.preventDefault()
    event.stopPropagation()
    setOpen(true)
  }
  return (
    <Tooltip open={open} onOpenChange={setOpen} delayDuration={350}>
      <TooltipTrigger
        asChild
        onClickCapture={explain}
        onKeyDownCapture={(event) => {
          if (event.key === 'Enter' || event.key === ' ') explain(event)
        }}
      >
        {children}
      </TooltipTrigger>
      <TooltipContent sideOffset={6} className="max-w-72 text-left leading-relaxed">
        {reason}
      </TooltipContent>
    </Tooltip>
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
        disabled={reason ? false : props.disabled}
        onClick={reason ? undefined : props.onClick}
        title={reason ? undefined : props.title}
        aria-disabled={reason ? true : undefined}
        aria-description={reason || undefined}
        className={cn('aria-disabled:cursor-not-allowed aria-disabled:opacity-50', props.className)}
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
        disabled={reason ? false : props.disabled}
        onCheckedChange={reason ? undefined : props.onCheckedChange}
        title={reason ? undefined : props.title}
        aria-disabled={reason ? true : undefined}
        aria-description={reason || undefined}
        className={cn('aria-disabled:cursor-not-allowed aria-disabled:opacity-50', props.className)}
      />
    </Availability>
  )
}
