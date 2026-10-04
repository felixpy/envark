import { execFileSync } from 'node:child_process'

export function gh(args) {
  return execFileSync('gh', args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim()
}

export function api(path) {
  return JSON.parse(gh(['api', `repos/${process.env.GITHUB_REPOSITORY}/${path}`]))
}

export function validateTag(tag) {
  if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error('Expected a stable release tag such as v0.2.0')
  return tag
}

export function tagCommit(tag) {
  let object = api(`git/ref/tags/${validateTag(tag)}`).object
  for (let depth = 0; object.type === 'tag' && depth < 5; depth++) {
    object = api(`git/tags/${object.sha}`).object
  }
  if (object.type !== 'commit' || !/^[a-f0-9]{40}$/.test(object.sha))
    throw new Error('Tag does not resolve to a commit')
  return object.sha
}
