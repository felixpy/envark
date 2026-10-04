import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { checkVersions } from './check-versions.mjs'
import { collectAssets, targets } from './publish-release.mjs'
import { findReleaseByTag, getReleaseById } from './release-github.mjs'

test('draft releases resolve without the published-only tag endpoint', () => {
  const draft = { id: 42, tag_name: 'v0.1.0', draft: true }
  const request = (path) => {
    if (path === 'releases?per_page=100&page=1') return [draft]
    if (path === 'releases/42') return draft
    throw new Error(`Not Found: ${path}`)
  }
  const found = findReleaseByTag('v0.1.0', request)
  assert.deepEqual(getReleaseById(found.id, 'v0.1.0', request), draft)
  assert.equal(findReleaseByTag('v0.2.0', request), undefined)
})

test('draft recovery searches beyond the first page of releases', () => {
  const draft = { id: 42, tag_name: 'v0.1.0', draft: true }
  const firstPage = Array.from({ length: 100 }, (_, i) => ({
    id: i + 100,
    tag_name: `v1.0.${i}`,
    draft: false,
  }))
  const request = (path) => {
    if (path === 'releases?per_page=100&page=1') return firstPage
    if (path === 'releases?per_page=100&page=2') return [draft]
    throw new Error(`Unexpected request: ${path}`)
  }
  assert.deepEqual(findReleaseByTag('v0.1.0', request), draft)
  assert.equal(findReleaseByTag('v0.2.0', request), undefined)
})

test('release validation rejects a changed identity or tag before publication', () => {
  const draft = { id: 42, tag_name: 'v0.1.0', draft: true }
  assert.throws(
    () => getReleaseById('42', 'v0.1.0', () => ({ ...draft, id: 43 })),
    /Release identity changed/,
  )
  assert.throws(
    () => getReleaseById('42', 'v0.1.0', () => ({ ...draft, tag_name: 'v0.2.0' })),
    /Release tag changed/,
  )
  for (const id of [undefined, '', '0', '-1', 'tags/v0.1.0']) {
    assert.throws(
      () => getReleaseById(id, 'v0.1.0', () => assert.fail('Invalid IDs must not reach the API')),
      /Expected a release ID/,
    )
  }
  const published = { ...draft, draft: false }
  assert.equal(getReleaseById('42', 'v0.1.0', () => published).draft, false)
})

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'envark-release-'))
  t.after(() => {
    assert.equal(dirname(root), resolve(tmpdir()))
    assert.ok(root.startsWith(join(tmpdir(), 'envark-release-')))
    rmSync(root, { recursive: true, force: true })
  })
  return root
}

function versionFixture(t) {
  const root = fixture(t)
  const files = [
    'package.json',
    'Cargo.toml',
    'Cargo.lock',
    'src-tauri/tauri.conf.json',
    '.release-please-manifest.json',
    'src-tauri/Cargo.toml',
    'crates/envark-core/Cargo.toml',
    'release-please-config.json',
  ]
  for (const file of files) {
    mkdirSync(dirname(join(root, file)), { recursive: true })
    writeFileSync(join(root, file), readFileSync(new URL(`../${file}`, import.meta.url)))
  }
  return root
}

test('release validation rejects version drift in every package and lockfile', (t) => {
  const root = versionFixture(t)
  const packageVersion = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version
  writeFileSync(
    join(root, '.release-please-manifest.json'),
    JSON.stringify({ '.': packageVersion }),
  )
  const version = checkVersions(root)
  assert.equal(checkVersions(root, `v${version}`), version)
  assert.throws(() => checkVersions(root, 'v999.0.0'))
  for (const file of [
    'package.json',
    'Cargo.toml',
    'src-tauri/tauri.conf.json',
    '.release-please-manifest.json',
  ]) {
    const path = join(root, file)
    const original = readFileSync(path, 'utf8')
    writeFileSync(path, original.replace(`"${version}"`, '"999.0.0"'))
    assert.throws(() => checkVersions(root), file)
    writeFileSync(path, original)
  }
  const lockPath = join(root, 'Cargo.lock')
  const originalLock = readFileSync(lockPath, 'utf8')
  for (const name of ['envark', 'envark-core']) {
    const changedLock = originalLock.replace(
      new RegExp(`(name = "${name}"\\r?\\nversion = ")[^"]+"`),
      (_, prefix) => `${prefix}999.0.0"`,
    )
    assert.notEqual(changedLock, originalLock, `Missing ${name} fixture entry`)
    writeFileSync(lockPath, changedLock)
    assert.throws(() => checkVersions(root), new RegExp(`${name} lockfile version drift`))
    writeFileSync(lockPath, originalLock)
  }
})

test('an empty release manifest is valid only for an untagged initial version', (t) => {
  const root = versionFixture(t)
  const manifestPath = join(root, '.release-please-manifest.json')
  const configPath = join(root, 'release-please-config.json')
  const version = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')).version
  const config = JSON.parse(readFileSync(configPath, 'utf8'))
  config['initial-version'] = version
  writeFileSync(configPath, JSON.stringify(config))
  writeFileSync(manifestPath, '{}')
  assert.equal(checkVersions(root), version)
  assert.throws(() => checkVersions(root, `v${version}`), /tagged release must record its version/)

  config['initial-version'] = '999.0.0'
  writeFileSync(configPath, JSON.stringify(config))
  assert.throws(() => checkVersions(root), /Initial release version drift/)

  writeFileSync(manifestPath, JSON.stringify({ unexpected: version }))
  assert.throws(() => checkVersions(root), /Only an empty release manifest may bootstrap/)
})

test('publishing requires all five correctly versioned platform installers', (t) => {
  const root = fixture(t)
  const version = '0.2.0'
  const paths = []
  for (const [target, extensions] of Object.entries(targets)) {
    const directory = join(root, `envark-${target}`)
    mkdirSync(directory)
    for (const extension of extensions) {
      const path = join(directory, `Envark_${version}_${target}${extension}`)
      writeFileSync(path, `fixture ${target}${extension}`)
      paths.push(path)
    }
  }
  const assets = collectAssets(root, version)
  assert.equal(assets.length, 5)
  assert.ok(assets.every((asset) => /^sha256:[a-f0-9]{64}$/.test(asset.digest)))
  assert.throws(() => collectAssets(root, '0.3.0'))
  rmSync(paths[0])
  assert.throws(() => collectAssets(root, version))
  writeFileSync(paths[0], '')
  assert.throws(() => collectAssets(root, version))
})
