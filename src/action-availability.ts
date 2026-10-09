import { updateKind, type Asset, type Manager, type Runtime, type Tool } from './domain'

type Translate = (zh: string, en: string) => string

export function runtimeRemovalReason(runtime: Runtime, t: Translate) {
  if (runtime.active)
    return t(
      '当前环境正在使用此版本，请先切换到其他版本。',
      'This version is active. Switch to another version before removing it.',
    )
  if (!runtime.activeKnown)
    return t(
      '无法确认此版本是否正在使用，请重新扫描。',
      'Active status is unknown. Rescan before removing this version.',
    )
  if (!runtime.managed)
    return (
      runtime.note ||
      t(
        '此版本由外部安装器管理，请通过原安装器卸载。',
        'This version is managed externally. Remove it through its original installer.',
      )
    )
  return null
}

export function runtimeDefaultReason(runtime: Runtime, managers: Manager[], t: Translate) {
  if (!runtime.managed)
    return (
      runtime.note ||
      t(
        '此版本未由支持的版本管理器管理。',
        'This version is not owned by a supported version manager.',
      )
    )
  if (!managers.some((m) => m.name === runtime.manager && m.supportsDefault))
    return t(
      '此管理器暂不支持切换默认版本。',
      'This manager does not support switching the default version.',
    )
  return null
}

export function assetRemovalReason(asset: Asset, t: Translate) {
  if (!asset.canRemove)
    return (
      asset.note ||
      t(
        '资源仍在使用或无法确认归属，请使用原工具管理。',
        'This resource is in use or its ownership is unverified. Manage it with the owning tool.',
      )
    )
  if (!asset.size.complete)
    return t(
      '资源扫描不完整，请检查权限后重新扫描。',
      'Resource scan is incomplete. Check permissions and rescan.',
    )
  return null
}

export function toolUpdateReason(tool: Tool, checkUpdates: boolean, t: Translate) {
  if (!tool.canUpdate)
    return (
      tool.note ||
      t(
        '尚未识别此工具的更新方式。',
        'The update method for this installation has not been identified.',
      )
    )
  switch (updateKind(tool)) {
    case 'unknown':
      return checkUpdates
        ? t(
            '尚未获取版本信息，请点击“重新检查更新”。',
            'No version information yet. Click Check updates again.',
          )
        : t('更新检查未开启，请先在设置中开启。', 'Enable update checks in Settings first.')
    case 'latest':
      return t('已是最新版本，无需更新。', 'Already up to date.')
    case 'ahead':
      return t(
        '当前版本高于已发布版本，不执行降级。',
        'The installed version is ahead of the published version.',
      )
    default:
      return null
  }
}
