import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, join, resolve } from 'node:path'
import { test } from 'node:test'
import { checkVersions } from './check-versions.mjs'
import { collectAssets, targets, updaterManifest } from './publish-release.mjs'
import { createHash, generateKeyPairSync, sign } from 'node:crypto'
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

test('publishing requires every installer, updater bundle, and signature', (t) => {
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
  assert.equal(assets.length, 11)
  assert.ok(assets.every((asset) => /^sha256:[a-f0-9]{64}$/.test(asset.digest)))
  assert.throws(() => collectAssets(root, '0.3.0'))
  rmSync(paths[0])
  assert.throws(() => collectAssets(root, version))
  writeFileSync(paths[0], '')
  assert.throws(() => collectAssets(root, version))
})

test('updater manifests verify signatures, distinguish macOS architectures, and reject tampering', (t) => {
  const root = fixture(t)
  const { publicKey, privateKey } = generateKeyPairSync('ed25519')
  const id = Buffer.from('0102030405060708', 'hex')
  const keyPacket = Buffer.concat([
    Buffer.from('Ed'),
    id,
    publicKey.export({ format: 'der', type: 'spki' }).subarray(-32),
  ])
  const encodedKey = Buffer.from(
    `untrusted comment: fixture\n${keyPacket.toString('base64')}\n`,
  ).toString('base64')
  for (const [target, extensions] of Object.entries(targets)) {
    const directory = join(root, `envark-${target}`)
    mkdirSync(directory)
    for (const extension of extensions.filter((e) => !e.endsWith('.sig'))) {
      const name =
        extension === '.app.tar.gz' ? 'Envark.app.tar.gz' : `Envark_0.3.0_${target}${extension}`
      const path = join(directory, name)
      const bytes = Buffer.from(`installer ${target}${extension}`)
      writeFileSync(path, bytes)
      if (extensions.includes(`${extension}.sig`)) {
        const artifactSignature = sign(
          null,
          createHash('blake2b512').update(bytes).digest(),
          privateKey,
        )
        const packet = Buffer.concat([Buffer.from('ED'), id, artifactSignature])
        const comment = 'timestamp:1700000000'
        const commentSignature = sign(
          null,
          Buffer.concat([artifactSignature, Buffer.from(comment)]),
          privateKey,
        )
        writeFileSync(
          `${path}.sig`,
          Buffer.from(
            `untrusted comment: fixture\n${packet.toString('base64')}\ntrusted comment: ${comment}\n${commentSignature.toString('base64')}\n`,
          ).toString('base64'),
        )
      }
    }
  }
  const assets = collectAssets(root, '0.3.0')
  const manifest = updaterManifest(
    assets,
    'v0.3.0',
    'example/envark',
    'Changes',
    '2026-10-06T00:00:00Z',
    encodedKey,
  )
  assert.equal(Object.keys(manifest.platforms).length, 4)
  assert.notEqual(manifest.platforms['darwin-aarch64'].url, manifest.platforms['darwin-x86_64'].url)
  assert.equal(manifest.version, '0.3.0')
  assert.throws(
    () =>
      updaterManifest(
        assets.filter((a) => !a.name.endsWith('.exe.sig')),
        'v0.3.0',
        'example/envark',
        '',
        '',
        encodedKey,
      ),
    /Missing updater signature/,
  )
  writeFileSync(assets.find((a) => a.name.endsWith('.exe')).path, 'tampered')
  assert.throws(
    () => updaterManifest(assets, 'v0.3.0', 'example/envark', '', '', encodedKey),
    /signature is invalid/,
  )
})

test(
  'macOS packaging checks reject missing resource seals and invalid signatures',
  {
    skip: process.platform === 'win32',
  },
  (t) => {
    const root = fixture(t)
    const bundle = join(root, 'bundle with spaces')
    const app = join(bundle, 'macos', 'Envark.app')
    const dmgApp = join(root, 'disk image payload', 'Envark.app')
    const sealPath = (path) => join(path, 'Contents', '_CodeSignature', 'CodeResources')
    for (const path of [app, dmgApp]) {
      mkdirSync(dirname(sealPath(path)), { recursive: true })
      writeFileSync(sealPath(path), 'fixture seal')
    }
    const archive = join(bundle, 'macos', 'Envark.app.tar.gz')
    const pack = () =>
      execFileSync('tar', ['-czf', archive, '-C', dirname(app), 'Envark.app'], {
        stdio: ['ignore', 'pipe', 'pipe'],
      })
    pack()
    mkdirSync(join(bundle, 'dmg'))
    writeFileSync(join(bundle, 'dmg', 'Envark.dmg'), 'fixture image')
    const bin = join(root, 'bin')
    const log = join(root, 'verification.log')
    mkdirSync(bin)
    // Apple tools are mocked on non-macOS hosts; real signatures are checked in CI.
    writeFileSync(
      join(bin, 'codesign'),
      `#!/usr/bin/env bash
set -eu
echo "codesign $*" >> "$VERIFY_LOG"
if [[ -n "$FAIL_COPY" && "$*" == *"$FAIL_COPY"* ]]; then exit 1; fi
`,
      { mode: 0o755 },
    )
    writeFileSync(
      join(bin, 'hdiutil'),
      `#!/usr/bin/env bash
set -eu
echo "hdiutil $*" >> "$VERIFY_LOG"
if [[ "$1" == attach ]]; then cp -R "$VERIFY_DMG_APP" "$6/Envark.app"; fi
`,
      { mode: 0o755 },
    )
    function run(extra = {}) {
      writeFileSync(log, '')
      return spawnSync('bash', [resolve('scripts/verify-macos-bundles.sh'), bundle], {
        encoding: 'utf8',
        stdio: ['ignore', 'pipe', 'pipe'],
        env: {
          ...process.env,
          PATH: `${bin}:${process.env.PATH}`,
          VERIFY_LOG: log,
          VERIFY_DMG_APP: dmgApp,
          FAIL_COPY: '',
          REQUIRE_UPDATER_ARCHIVE: 'true',
          ...extra,
        },
      })
    }
    const success = run()
    assert.equal(success.status, 0, success.stderr)
    const commands = readFileSync(log, 'utf8')
    assert.equal(commands.split('\n').filter((line) => line.startsWith('codesign ')).length, 3)
    assert.match(commands, /hdiutil detach/)
    for (const copy of ['/macos/', '/updater/', '/dmg/']) {
      assert.notEqual(run({ FAIL_COPY: copy }).status, 0)
      if (copy === '/dmg/') assert.match(readFileSync(log, 'utf8'), /hdiutil detach/)
      else assert.doesNotMatch(readFileSync(log, 'utf8'), /hdiutil attach/)
    }
    rmSync(sealPath(app))
    assert.match(run().stderr, /Missing application resource seal/)
    pack()
    writeFileSync(sealPath(app), 'fixture seal')
    assert.match(run().stderr, /Missing application resource seal/)
    pack()
    rmSync(sealPath(dmgApp))
    assert.match(run().stderr, /Missing application resource seal/)
    assert.match(readFileSync(log, 'utf8'), /hdiutil detach/)
    writeFileSync(sealPath(dmgApp), 'fixture seal')
    rmSync(archive)
    assert.notEqual(run().status, 0, 'Release builds require an updater archive')
    assert.equal(run({ REQUIRE_UPDATER_ARCHIVE: 'false' }).status, 0)
  },
)
