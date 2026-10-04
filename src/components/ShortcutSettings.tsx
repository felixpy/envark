import { useState } from 'react'
import { useStore } from '@/store'
import { shortcutGroups, shortcutLabel, type ShortcutId } from '@/shortcuts'
import { Button } from './ui/button'
import { Switch } from './ui/switch'

export function ShortcutSettings() {
  const { data, setDisabledShortcuts, t } = useStore()
  const [saving, setSaving] = useState(false)
  const disabled = data.settings.disabledShortcuts
  const save = async (shortcuts: ShortcutId[]) => {
    setSaving(true)
    try {
      await setDisabledShortcuts(shortcuts)
    } finally {
      setSaving(false)
    }
  }
  return (
    <>
      <div className="max-h-[min(32rem,60dvh)] space-y-5 overflow-y-auto px-6 pb-5">
        {shortcutGroups.map((group) => (
          <section key={group.id} aria-labelledby={`shortcut-group-${group.id}`}>
            <h3
              id={`shortcut-group-${group.id}`}
              className="py-2 text-xs font-medium text-muted-foreground"
            >
              {t(group.label[0], group.label[1])}
            </h3>
            <div className="divide-y">
              {group.shortcuts.map((shortcut) => {
                const enabled = !disabled.includes(shortcut.id)
                const label = t(shortcut.label[0], shortcut.label[1])
                return (
                  <div key={shortcut.id} className="flex min-h-12 items-center gap-3 py-3">
                    <Switch
                      id={`shortcut-${shortcut.id}`}
                      checked={enabled}
                      disabled={saving}
                      onCheckedChange={(checked) =>
                        void save(
                          checked
                            ? disabled.filter((id) => id !== shortcut.id)
                            : [...disabled, shortcut.id],
                        )
                      }
                    />
                    <label
                      htmlFor={`shortcut-${shortcut.id}`}
                      className="flex-1 cursor-pointer text-sm"
                    >
                      {label}
                    </label>
                    <kbd
                      className={`shrink-0 rounded-md bg-muted px-2 py-1 font-sans text-xs ${enabled ? 'text-muted-foreground' : 'text-muted-foreground/50'}`}
                    >
                      {shortcutLabel(shortcut.key, data.platform)}
                    </kbd>
                  </div>
                )
              })}
            </div>
          </section>
        ))}
      </div>
      <div className="flex justify-end border-t px-6 py-4">
        <Button
          variant="outline"
          size="sm"
          disabled={saving || disabled.length === 0}
          onClick={() => void save([])}
        >
          {t('恢复默认快捷键', 'Restore default shortcuts')}
        </Button>
      </div>
    </>
  )
}
