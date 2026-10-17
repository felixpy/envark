import type { Tool } from '@/domain'

type Translate = (zh: string, en: string) => string

export function retryableUpdateIssue(tool: Tool) {
  return (
    tool.updateIssue?.reason === 'queryFailed' &&
    !/401|403|404|unauthorized|forbidden|not.found|authentication|certificate|invalid.+config|direct.source|Git,|local source/i.test(
      tool.updateIssue.detail,
    )
  )
}

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
  if (/401|unauthorized|authentication/i.test(detail))
    return t('需要验证仓库凭据', 'Registry authentication required')
  if (/404|not.found|E404/i.test(detail))
    return t('包不存在或无权访问', 'Package unavailable or access denied')
  if (/timed?\s*out|timeout/i.test(detail)) return t('更新源响应超时', 'Update source timed out')
  if (/certificate|tls|ssl/i.test(detail))
    return t('安全连接验证失败', 'Secure connection validation failed')
  if (/429|rate.limit/i.test(detail)) return t('更新源暂时限流', 'Update source is rate limited')
  if (/403|forbidden/i.test(detail))
    return t('更新源拒绝请求', 'Update source rejected the request')
  if (/connect|resolve|dns|network/i.test(detail))
    return t('无法连接更新源', 'Cannot connect to the update source')
  return t('版本查询失败', 'Version lookup failed')
}
