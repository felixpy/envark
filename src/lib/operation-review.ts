import type { ActionRequest, OperationResult } from '@/domain'

export function failedReview(
  request: ActionRequest | null,
  result: OperationResult | null,
): ActionRequest | null {
  if (!request || !result) return null
  const failed = result.items.filter((item) => item.status === 'failed')
  if (!failed.length) return null
  const ids = new Set(failed.flatMap((item) => item.targetIds ?? []))
  switch (request.kind) {
    case 'cleanCaches':
    case 'updateTools':
    case 'removeWorktrees':
    case 'removeAssets': {
      const selected = request.ids.filter((id) => ids.has(id))
      return selected.length ? { ...request, ids: selected } : null
    }
    case 'cleanProjects': {
      const selected = request.artifactIds.filter((id) => ids.has(id))
      return selected.length ? { ...request, artifactIds: selected } : null
    }
    default:
      return request
  }
}
