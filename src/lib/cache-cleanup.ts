import type { Cache } from '@/domain'

type Translate = (zh: string, en: string) => string

export function cleanupAvailability(cache: Cache, t: Translate) {
  const issue = cache.cleanupIssue
  if (!issue) return null
  switch (issue.reason) {
    case 'toolUnavailable':
      return t(
        '尚未找到可用的清理工具，请在环境与工具中安装或修复。',
        'No usable cleanup tool was found. Install or repair it in Environments & tools.',
      )
    case 'unsupportedTool':
      return t(
        '当前工具版本不支持此清理方式，请先更新工具。',
        'This tool version does not support cleanup. Update the tool first.',
      )
    case 'busy':
      return t(
        '缓存正在被构建或下载任务使用。任务结束后重新检查即可。',
        'A build or download is using this cache. Check again after it finishes.',
      )
    case 'localChanges':
      return t(
        '缓存中有本地修改，已保留这些内容。',
        'Local changes were found in the cache and are preserved.',
      )
    case 'changed':
      return t(
        '缓存路径或工具配置已变化，请刷新缓存重新发现。',
        'The cache path or tool configuration changed. Refresh caches to discover it again.',
      )
    case 'accessDenied':
      return t(
        '当前用户无权访问此缓存。恢复目录权限后重新检查。',
        'The current user cannot access this cache. Check again after restoring directory access.',
      )
    case 'unsafePath':
      return t(
        '此路径无法确认属于可清理缓存，已保留。',
        'This path cannot be verified as a cleanup target and is preserved.',
      )
    default:
      return t(
        '清理工具暂未通过检查，请查看具体原因并重新检查。',
        'The cleanup tool could not be verified. Inspect the details and check again.',
      )
  }
}

export function cleanupEnvironmentTab(cache: Cache): 'runtime' | 'pm' {
  if (
    cache.cleanupIssue?.detail.startsWith('java ') ||
    cache.cleanupIssue?.detail.startsWith('Java ') ||
    cache.cleanupIssue?.detail.startsWith('node ') ||
    cache.name === 'npm' ||
    cache.cleanupIssue?.detail.startsWith('python')
  )
    return 'runtime'
  return 'pm'
}
