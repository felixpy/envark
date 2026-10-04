import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { api, gh, tagCommit, validateTag } from './release-github.mjs'

export const targets = {
  'x86_64-pc-windows-msvc': ['.exe'],
  'aarch64-apple-darwin': ['.dmg'],
  'x86_64-apple-darwin': ['.dmg'],
  'x86_64-unknown-linux-gnu': ['.deb', '.AppImage'],
}

function filesIn(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    assert.ok(!entry.isSymbolicLink(), 'Release artifacts must not be links')
    const path = join(directory, entry.name)
    return entry.isDirectory() ? filesIn(path) : [path]
  })
}

export function collectAssets(root, version) {
  const assets = []
  const names = new Set()
  for (const [target, extensions] of Object.entries(targets)) {
    const files = filesIn(join(root, `envark-${target}`))
    assert.equal(files.length, extensions.length, `Unexpected installer count for ${target}`)
    for (const extension of extensions) {
      const matching = files.filter((file) => file.endsWith(extension))
      assert.equal(matching.length, 1, `Expected one ${extension} installer for ${target}`)
      const path = matching[0]
      const name = basename(path)
      assert.ok(name.includes(`_${version}_`), `Installer version mismatch: ${name}`)
      assert.ok(
        !/[\r\n]/.test(name) && !names.has(name),
        `Invalid or duplicate asset name: ${name}`,
      )
      names.add(name)
      const bytes = readFileSync(path)
      assert.ok(bytes.length > 0, `Empty installer: ${name}`)
      assets.push({
        path,
        name,
        size: bytes.length,
        digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`,
      })
    }
  }
  return assets.sort((a, b) => a.name.localeCompare(b.name, 'en'))
}

function publish() {
  const tag = validateTag(process.env.RELEASE_TAG)
  assert.equal(tagCommit(tag), process.env.RELEASE_SHA, 'The tag changed after the build')
  const release = api(`releases/tags/${tag}`)
  assert.equal(String(release.id), process.env.RELEASE_ID, 'Draft release identity changed')
  if (!release.draft) {
    console.log(`${tag} is already published; leaving it unchanged.`)
    return
  }
  const assets = collectAssets('packages', tag.slice(1))
  const checksumPath = join('packages', 'SHA256SUMS')
  writeFileSync(
    checksumPath,
    assets.map(({ name, digest }) => `${digest.slice(7)}  ${name}\n`).join(''),
  )
  const bytes = readFileSync(checksumPath)
  assets.push({
    path: checksumPath,
    name: 'SHA256SUMS',
    size: bytes.length,
    digest: `sha256:${createHash('sha256').update(bytes).digest('hex')}`,
  })
  gh(['release', 'upload', tag, '--clobber', ...assets.map((asset) => asset.path)])
  const uploaded = api(`releases/${release.id}/assets?per_page=100`)
  assert.equal(uploaded.length, assets.length, 'Unexpected assets on the draft release')
  for (const asset of assets) {
    const remote = uploaded.find((item) => item.name === asset.name)
    assert.ok(
      remote && remote.state === 'uploaded' && remote.size === asset.size,
      `Upload verification failed: ${asset.name}`,
    )
    if (remote.digest) assert.equal(remote.digest, asset.digest, `Checksum mismatch: ${asset.name}`)
  }
  assert.equal(tagCommit(tag), process.env.RELEASE_SHA, 'The tag changed during upload')
  gh(['release', 'edit', tag, '--draft=false'])
  console.log(`Published ${tag} with all platform installers and SHA256SUMS.`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) publish()
