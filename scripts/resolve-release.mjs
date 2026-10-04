import { appendFileSync, readFileSync } from 'node:fs'
import { execFileSync } from 'node:child_process'
import { findReleaseByTag, getReleaseById, tagCommit, validateTag } from './release-github.mjs'

const requested = process.env.NEW_TAG || process.env.RETRY_TAG
const candidate = validateTag(
  requested || `v${JSON.parse(readFileSync('package.json', 'utf8')).version}`,
)
const release = findReleaseByTag(candidate)
if (!release) {
  if (requested) throw new Error(`No existing draft release for ${candidate}`)
  console.log('No draft release is ready to build.')
} else if (!release.draft) {
  console.log(`${candidate} is already published; leaving it unchanged.`)
} else {
  const sha = tagCommit(candidate)
  if (process.env.NEW_SHA && sha !== process.env.NEW_SHA)
    throw new Error('Release commit and tag disagree')
  execFileSync('git', ['merge-base', '--is-ancestor', sha, 'origin/main'], { stdio: 'inherit' })
  const detail = getReleaseById(release.id, candidate)
  if (!detail.draft) throw new Error('The draft was published during validation')
  appendFileSync(
    process.env.GITHUB_OUTPUT,
    `tag=${candidate}\nsha=${sha}\nrelease_id=${detail.id}\n`,
  )
  console.log(`Building ${candidate} from ${sha}`)
}
