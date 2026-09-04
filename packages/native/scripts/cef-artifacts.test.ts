import { afterEach, describe, expect, test } from 'bun:test'
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { readCefArtifactInventory, verifyCefArtifactInventory } from './cef-artifacts.ts'

const temporaryRoots: string[] = []

afterEach(() => {
  for (const root of temporaryRoots.splice(0)) rmSync(root, { recursive: true, force: true })
})

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), 'gpuix-cef-artifacts-'))
  temporaryRoots.push(root)
  mkdirSync(join(root, 'empty'))
  writeFileSync(join(root, 'helper'), 'binary', { mode: 0o755 })
  return root
}

describe('CEF artifact inventory', () => {
  test('records empty directories and rejects inherited-name extras', () => {
    const root = fixture()
    const inventory = readCefArtifactInventory(root)
    expect(inventory.empty).toEqual({ type: 'directory', mode: 0o755 })

    writeFileSync(join(root, 'constructor'), 'unexpected')
    expect(() => verifyCefArtifactInventory(root, inventory)).toThrow('extra: constructor')
  })

  test('rejects special permission-bit changes', () => {
    const root = fixture()
    const inventory = readCefArtifactInventory(root)

    chmodSync(join(root, 'helper'), 0o4755)

    expect(() => verifyCefArtifactInventory(root, inventory)).toThrow('does not match its manifest: helper')
  })
})
