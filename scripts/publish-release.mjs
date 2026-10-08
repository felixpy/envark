import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { copyFileSync, readFileSync, readdirSync, writeFileSync } from 'node:fs'
import { basename, join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { api, getReleaseById, gh, tagCommit, validateTag } from './release-github.mjs'
import { verifyUpdaterSignature } from './updater-signature.mjs'

export const targets = {
  'x86_64-pc-windows-msvc': ['.exe', '.exe.sig'],
  'aarch64-apple-darwin': ['.dmg', '.app.tar.gz', '.app.tar.gz.sig'],
  'x86_64-apple-darwin': ['.dmg', '.app.tar.gz', '.app.tar.gz.sig'],
  'x86_64-unknown-linux-gnu': ['.deb', '.AppImage', '.AppImage.sig'],
}

const updaterTargets = {
  'windows-x86_64': ['x86_64-pc-windows-msvc', '.exe'],
  'darwin-aarch64': ['aarch64-apple-darwin', '.app.tar.gz'],
  'darwin-x86_64': ['x86_64-apple-darwin', '.app.tar.gz'],
  'linux-x86_64': ['x86_64-unknown-linux-gnu', '.AppImage'],
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
      const originalName = basename(path)
      // Tauri's macOS updater archive has no version or architecture in its name.
      const name = extension.startsWith('.app.tar.gz')
        ? `Envark_${version}_${target}${extension}`
        : originalName
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
        target,
      })
    }
  }
  return assets.sort((a, b) => a.name.localeCompare(b.name, 'en'))
}

export function updaterManifest(assets, tag, repository, notes, publishedAt, publicKey) {
  validateTag(tag)
  assert.match(repository, /^[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$/)
  const platforms = {}
  for (const [platform, [target, extension]] of Object.entries(updaterTargets)) {
    const asset = assets.find((a) => a.target === target && a.name.endsWith(extension))
    assert.ok(asset, `Missing updater artifact for ${platform}`)
    const signed = assets.find((a) => a.target === target && a.name === `${asset.name}.sig`)
    assert.ok(signed, `Missing updater signature for ${platform}`)
    const signature = readFileSync(signed.path, 'utf8').trim()
    assert.ok(signature.length > 0 && signature.length < 16384, `Invalid signature for ${platform}`)
    verifyUpdaterSignature(readFileSync(asset.path), signature, publicKey)
    platforms[platform] = {
      signature,
      url: `https://github.com/${repository}/releases/download/${tag}/${encodeURIComponent(asset.name)}`,
    }
  }
  return { version: tag.slice(1), notes, pub_date: publishedAt, platforms }
}

export function prepareReleaseAssets(root, tag, repository, notes, publishedAt, publicKey) {
  validateTag(tag)
  const buildAssets = collectAssets(root, tag.slice(1))
  const manifestPath = join(root, 'latest.json')
  writeFileSync(
    manifestPath,
    JSON.stringify(
      updaterManifest(buildAssets, tag, repository, notes, publishedAt, publicKey),
      null,
      2,
    ) + '\n',
  )
  // Sidecars remain build inputs; clients read their verified signatures from latest.json.
  const assets = buildAssets.filter((asset) => !asset.name.endsWith('.sig'))
  const manifestBytes = readFileSync(manifestPath)
  assets.push({
    path: manifestPath,
    name: 'latest.json',
    size: manifestBytes.length,
    digest: `sha256:${createHash('sha256').update(manifestBytes).digest('hex')}`,
  })
  for (const asset of assets) {
    if (basename(asset.path) !== asset.name) {
      const destination = join(root, asset.name)
      copyFileSync(asset.path, destination)
      asset.path = destination
    }
  }
  const checksumPath = join(root, 'SHA256SUMS')
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
  return assets
}

export function verifyReleaseUploads(uploaded, assets) {
  // A resumed draft may still contain sidecars uploaded by the older publisher.
  const obsoleteNames = new Set(
    Object.values(updaterTargets).map(([target, extension]) => {
      const asset = assets.find((a) => a.target === target && a.name.endsWith(extension))
      assert.ok(asset, `Missing updater artifact for ${target}`)
      return `${asset.name}.sig`
    }),
  )
  const obsolete = uploaded.filter((asset) => obsoleteNames.has(asset.name))
  assert.equal(
    uploaded.length - obsolete.length,
    assets.length,
    'Unexpected assets on the draft release',
  )
  for (const asset of assets) {
    const remote = uploaded.find((item) => item.name === asset.name)
    assert.ok(
      remote && remote.state === 'uploaded' && remote.size === asset.size,
      `Upload verification failed: ${asset.name}`,
    )
    if (remote.digest) assert.equal(remote.digest, asset.digest, `Checksum mismatch: ${asset.name}`)
  }
  return obsolete
}

function publish() {
  const tag = validateTag(process.env.RELEASE_TAG)
  assert.equal(tagCommit(tag), process.env.RELEASE_SHA, 'The tag changed after the build')
  const release = getReleaseById(process.env.RELEASE_ID, tag)
  if (!release.draft) {
    console.log(`${tag} is already published; leaving it unchanged.`)
    return
  }
  const publicKey = JSON.parse(readFileSync('src-tauri/tauri.conf.json', 'utf8')).plugins.updater
    .pubkey
  const assets = prepareReleaseAssets(
    'packages',
    tag,
    process.env.GH_REPO,
    release.body ?? '',
    new Date().toISOString(),
    publicKey,
  )
  gh(['release', 'upload', tag, '--clobber', ...assets.map((asset) => asset.path)])
  const uploaded = api(`releases/${release.id}/assets?per_page=100`)
  const obsolete = verifyReleaseUploads(uploaded, assets)
  for (const asset of obsolete) gh(['release', 'delete-asset', tag, asset.name, '--yes'])
  if (obsolete.length > 0) {
    assert.equal(
      verifyReleaseUploads(api(`releases/${release.id}/assets?per_page=100`), assets).length,
      0,
      'Obsolete signature assets remain on the draft release',
    )
  }
  assert.equal(tagCommit(tag), process.env.RELEASE_SHA, 'The tag changed during upload')
  gh(['release', 'edit', tag, '--draft=false'])
  console.log(`Published ${tag} with all platform installers and SHA256SUMS.`)
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) publish()
