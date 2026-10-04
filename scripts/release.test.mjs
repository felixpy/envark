import assert from 'node:assert/strict'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { checkVersions } from './check-versions.mjs'
import { collectAssets, targets } from './publish-release.mjs'

function fixture(t) {
  const root = mkdtempSync(join(tmpdir(), 'envark-release-'))
  t.after(() => {
    assert.equal(dirname(root), resolve(tmpdir()))
    assert.ok(root.startsWith(join(tmpdir(), 'envark-release-')))
    rmSync(root, { recursive: true, force: true })
  })
  return root
}

test('release validation rejects version drift in every package and lockfile', (t) => {
  const root = fixture(t)
  const files = [
    'package.json',
    'Cargo.toml',
    'Cargo.lock',
    'src-tauri/tauri.conf.json',
    '.release-please-manifest.json',
    'src-tauri/Cargo.toml',
    'crates/envark-core/Cargo.toml',
  ]
  for (const file of files) {
    mkdirSync(dirname(join(root, file)), { recursive: true })
    writeFileSync(join(root, file), readFileSync(new URL(`../${file}`, import.meta.url)))
  }
  const version = checkVersions(root)
  assert.equal(checkVersions(root, `v${version}`), version)
  assert.throws(() => checkVersions(root, 'v999.0.0'))
  for (const file of files.slice(0, 5)) {
    const path = join(root, file)
    const original = readFileSync(path, 'utf8')
    writeFileSync(path, original.replace(`"${version}"`, '"999.0.0"'))
    assert.throws(() => checkVersions(root), file)
    writeFileSync(path, original)
  }
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
