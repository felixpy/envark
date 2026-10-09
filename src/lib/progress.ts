import { metadata, type ActionRequest, type Progress } from '@/domain'

type Translate = (zh: string, en: string) => string

export function progressLabel(progress: Progress | null, t: Translate, executing = false): string {
  if (!progress)
    return executing ? t('正在执行操作', 'Executing operations') : t('正在扫描', 'Scanning')
  switch (progress.stage) {
    case 'prepare':
      return t('正在检查所选项目，准备操作预览', 'Checking selected items before review')
    case 'refresh-affected':
      return t('操作已结束，正在更新受影响的项目', 'Operation finished; updating affected items')
    case 'refresh-environments':
      return t('操作已结束，正在更新相关环境', 'Operation finished; updating affected environments')
    case 'environments':
      return t('正在检测开发环境', 'Detecting development environments')
    case 'discover':
      return `${t('正在扫描项目', 'Scanning projects')} · ${progress.completed} ${t('个路径已检查', 'paths checked')}`
    case 'reuse':
      return `${t('正在复用扫描结果', 'Reusing scan results')} · ${progress.completed} ${t('个目录', 'folders')}`
    case 'measure-projects':
      return t('正在计算项目产物占用', 'Measuring project artifacts')
    case 'measure-worktrees':
      return t('正在计算工作树占用', 'Measuring worktrees')
    case 'projects-complete':
      return `${t('项目扫描完成', 'Project scan complete')} · ${progress.completed} ${t('个项目', 'projects')}`
    case 'updates':
      return t('正在检查工具更新', 'Checking tool updates')
    case 'measure-caches':
      return t('正在计算缓存占用', 'Measuring caches')
    case 'complete':
      return t('刷新完成', 'Refresh complete')
    case 'execute':
      return `${t('正在执行操作', 'Executing operations')} · ${progress.completed}${progress.total === null ? '' : `/${progress.total}`} · ${progress.message}`
    default:
      return progress.message || t('正在处理', 'Working')
  }
}

export function progressDetail(progress: Progress | null): string {
  if (!progress) return ''
  return [
    'prepare',
    'measure-projects',
    'measure-worktrees',
    'measure-caches',
    'refresh-affected',
  ].includes(progress.stage)
    ? progress.message
    : ''
}

export function operationLabel(request: ActionRequest | null, t: Translate): string {
  if (!request) return ''
  const provider = 'provider' in request ? metadata[request.provider].name : ''
  switch (request.kind) {
    case 'serviceAction':
      return `${request.action === 'start' ? t('启动服务', 'Start service') : t('停止服务', 'Stop service')} · ${provider}`
    case 'installManager':
      return request.provider === 'ollama'
        ? t('安装 Ollama', 'Install Ollama')
        : `${t('安装管理器', 'Install manager')} · ${request.manager}`
    case 'installRuntime':
      return `${t('安装版本', 'Install runtime')} · ${provider} · ${request.version}`
    case 'removeRuntime':
      return `${t('卸载版本', 'Uninstall runtime')} · ${provider}`
    case 'setDefault':
      return `${t('设置默认版本', 'Set default runtime')} · ${provider}`
    case 'updateTool':
    case 'updateTools':
      return `${t('更新工具', 'Update tools')} · ${provider}`
    case 'removeTool':
      return `${t('卸载工具', 'Uninstall tool')} · ${provider}`
    case 'downloadAsset':
      return `${t('下载资源', 'Download resource')} · ${provider}`
    case 'removeAssets':
      return `${t('移除资源', 'Remove resources')} · ${provider}`
    case 'cleanProjects':
      return t('清理项目产物', 'Clean project artifacts')
    case 'cleanCaches':
      return t('清理共享缓存', 'Clean shared caches')
    case 'removeWorktree':
    case 'removeWorktrees':
      return t('移除工作树', 'Remove worktrees')
  }
}
