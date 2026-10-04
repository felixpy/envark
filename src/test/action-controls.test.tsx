import { expect, it, vi } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ActionButton, SelectionCheckbox } from '@/components/action-controls'
import { TooltipProvider } from '@/components/ui/tooltip'

it('explains an unavailable action on hover, focus, and activation without executing it', async () => {
  const action = vi.fn()
  const user = userEvent.setup()
  render(
    <ActionButton reason="This version is in use." onClick={action}>
      Remove
    </ActionButton>,
    { wrapper: TooltipProvider },
  )
  const button = screen.getByRole('button', { name: 'Remove' })
  expect(screen.queryByRole('tooltip')).toBeNull()
  await user.hover(button)
  expect((await screen.findByRole('tooltip')).textContent).toBe('This version is in use.')
  await user.unhover(button)
  // jsdom has no layout, so move outside the tooltip's hover corridor explicitly.
  await user.pointer({ target: document.body, coords: { clientX: 500, clientY: 500 } })
  await waitFor(() => expect(screen.queryByRole('tooltip')).toBeNull())
  await user.tab()
  expect(document.activeElement).toBe(button)
  await screen.findByRole('tooltip')
  await user.keyboard('{Enter} ')
  await user.click(button)
  expect(action).not.toHaveBeenCalled()
  await user.keyboard('{Escape}')
  await waitFor(() => expect(screen.queryByRole('tooltip')).toBeNull())
})

it('never changes an unavailable checkbox through pointer or keyboard activation', async () => {
  const change = vi.fn()
  const user = userEvent.setup()
  const view = render(
    <SelectionCheckbox
      aria-label="Select protected"
      reason="Project is protected."
      onCheckedChange={change}
    />,
    { wrapper: TooltipProvider },
  )
  const checkbox = screen.getByRole('checkbox', { name: 'Select protected' })
  await user.click(checkbox)
  await screen.findByRole('tooltip')
  await user.keyboard(' {Enter}')
  expect(change).not.toHaveBeenCalled()
  expect(checkbox.getAttribute('aria-checked')).toBe('false')
  view.rerender(<SelectionCheckbox aria-label="Select protected" onCheckedChange={change} />)
  await user.click(screen.getByRole('checkbox', { name: 'Select protected' }))
  expect(change).toHaveBeenCalledExactlyOnceWith(true)
})
