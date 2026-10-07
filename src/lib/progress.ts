import type { Progress } from '@/domain'

type Translate = (zh: string, en: string) => string

export function progressLabel(progress: Progress | null, t: Translate, executing = false): string {
  if (!progress)
    return executing ? t('正在执行操作', 'Executing operations') : t('正在扫描', 'Scanning')
  switch (progress.stage) {
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
