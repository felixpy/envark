import { execFileSync } from 'node:child_process'
import assert from 'node:assert/strict'

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

export function findReleaseByTag(tag, request = api) {
  validateTag(tag)
  // The tag endpoint only returns published releases; the list includes accessible drafts.
  for (let page = 1; ; page++) {
    const releases = request(`releases?per_page=100&page=${page}`)
    const release = releases.find((release) => release.tag_name === tag)
    if (release) return release
    if (releases.length < 100) return undefined
  }
}

export function getReleaseById(id, tag, request = api) {
  assert.match(String(id), /^[1-9]\d*$/, 'Expected a release ID')
  validateTag(tag)
  const release = request(`releases/${id}`)
  assert.equal(String(release.id), String(id), 'Release identity changed')
  assert.equal(release.tag_name, tag, 'Release tag changed')
  return release
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
