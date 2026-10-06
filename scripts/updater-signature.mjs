import assert from 'node:assert/strict'
import { createHash, createPublicKey, verify } from 'node:crypto'

/** Verify Tauri's base64-encoded minisign envelope before publishing an update. */
export function verifyUpdaterSignature(bytes, signature, publicKey) {
  const keyLines = Buffer.from(publicKey, 'base64').toString('utf8').trim().split('\n')
  const lines = Buffer.from(signature, 'base64').toString('utf8').trim().split('\n')
  assert.equal(keyLines.length, 2, 'Invalid updater public key')
  assert.equal(lines.length, 4, 'Invalid updater signature envelope')
  const packet = Buffer.from(lines[1], 'base64')
  const keyPacket = Buffer.from(keyLines[1], 'base64')
  assert.equal(keyPacket.length, 42, 'Invalid updater public key length')
  assert.equal(packet.length, 74, 'Invalid updater signature length')
  assert.ok(
    packet.subarray(2, 10).equals(keyPacket.subarray(2, 10)),
    'Updater signing key mismatch',
  )
  const algorithm = packet.subarray(0, 2).toString('ascii')
  assert.ok(algorithm === 'ED' || algorithm === 'Ed', 'Unsupported updater signature algorithm')
  const key = createPublicKey({
    key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), keyPacket.subarray(10)]),
    format: 'der',
    type: 'spki',
  })
  const signedBytes = algorithm === 'ED' ? createHash('blake2b512').update(bytes).digest() : bytes
  assert.ok(
    verify(null, signedBytes, key, packet.subarray(10)),
    'Updater artifact signature is invalid',
  )
  assert.ok(lines[2].startsWith('trusted comment: '), 'Missing updater trusted comment')
  assert.ok(
    verify(
      null,
      Buffer.concat([packet.subarray(10), Buffer.from(lines[2].slice(17))]),
      key,
      Buffer.from(lines[3], 'base64'),
    ),
    'Updater comment signature is invalid',
  )
}
