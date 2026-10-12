import type { Tool } from '@/domain'

type Translate = (zh: string, en: string) => string

export function followsRustToolchain(tool: Tool) {
  return tool.name === 'cargo' && tool.source === 'rustup'
}

export function updateIssueLabel(tool: Tool, t: Translate): string | null {
  if (followsRustToolchain(tool) || tool.updateIssue?.reason === 'runtimeManaged')
    return t('随 Rust 工具链更新', 'Updates with the Rust toolchain')
  if (tool.updateIssue?.reason === 'unsupported')
    return t('更新来源尚未确认', 'Update source is unverified')
  if (tool.updateIssue?.reason === 'invalidVersion')
    return t('版本格式无法比较', 'Version formats cannot be compared')
  if (tool.updateIssue?.reason !== 'queryFailed') return null
  const detail = tool.updateIssue.detail
  if (/timed?\s*out|timeout/i.test(detail))
    return t('官方来源响应超时', 'Official source timed out')
  if (/certificate|tls|ssl/i.test(detail))
    return t('安全连接验证失败', 'Secure connection validation failed')
  if (/429|rate.limit/i.test(detail))
    return t('官方来源暂时限流', 'Official source is rate limited')
  if (/403|forbidden/i.test(detail))
    return t('官方来源拒绝请求', 'Official source rejected the request')
  if (/connect|resolve|dns|network/i.test(detail))
    return t('无法连接官方来源', 'Cannot connect to the official source')
  return t('版本查询失败', 'Version lookup failed')
}
