import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

export function checkVersions(root = process.cwd(), tag = '') {
  const read = (file) => readFileSync(resolve(root, file), 'utf8')
  const json = (file) => JSON.parse(read(file))
  const version = json('package.json').version
  assert.match(version, /^\d+\.\d+\.\d+$/, 'Desktop releases require a stable semantic version')
  if (tag) assert.equal(tag, `v${version}`, 'Tag and application version differ')
  assert.equal(json('src-tauri/tauri.conf.json').version, version, 'Tauri version drift')
  const releaseManifest = json('.release-please-manifest.json')
  if (releaseManifest['.'] === undefined) {
    assert.deepEqual(releaseManifest, {}, 'Only an empty release manifest may bootstrap')
    assert.ok(!tag, 'A tagged release must record its version in the release manifest')
    const config = json('release-please-config.json')
    assert.equal(
      config.packages['.']['initial-version'] ?? config['initial-version'],
      version,
      'Initial release version drift',
    )
  } else {
    assert.equal(releaseManifest['.'], version, 'Release manifest version drift')
  }
  const workspace = read('Cargo.toml')
    .split(/(?=^\[)/m)
    .find((section) => section.startsWith('[workspace.package]'))
  assert.ok(workspace, 'Missing workspace.package section')
  assert.equal(
    workspace.match(/^version\s*=\s*"([^"]+)"/m)?.[1],
    version,
    'Cargo workspace version drift',
  )
  const packages = read('Cargo.lock').split('[[package]]').slice(1)
  for (const [name, manifest] of [
    ['envark', 'src-tauri/Cargo.toml'],
    ['envark-core', 'crates/envark-core/Cargo.toml'],
  ]) {
    assert.match(
      read(manifest),
      /^version\.workspace\s*=\s*true\s*$/m,
      `${name} must inherit the workspace version`,
    )
    const entries = packages.filter((entry) => entry.match(/^name\s*=\s*"([^"]+)"/m)?.[1] === name)
    assert.equal(entries.length, 1, `Expected one ${name} lockfile entry`)
    assert.equal(
      entries[0].match(/^version\s*=\s*"([^"]+)"/m)?.[1],
      version,
      `${name} lockfile version drift`,
    )
  }
  return version
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  console.log(
    `Release versions agree: ${checkVersions(process.cwd(), process.argv[2] || process.env.RELEASE_TAG)}`,
  )
}
