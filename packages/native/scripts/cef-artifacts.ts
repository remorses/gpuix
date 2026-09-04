import { lstatSync, readdirSync, readlinkSync, realpathSync } from 'node:fs'
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path'

export type CefArtifactEntry =
  | { type: 'directory'; mode: number }
  | { type: 'file'; mode: number; size: number; sha256: string }
  | { type: 'symlink'; target: string }

export type CefArtifactInventory = Record<string, CefArtifactEntry>

export function readCefArtifactInventory(root: string): CefArtifactInventory {
  const inventory = Object.create(null) as CefArtifactInventory
  visit(root)
  return inventory

  function visit(directory: string): void {
    const entries = readdirSync(directory, { withFileTypes: true })
      .sort((left, right) => left.name < right.name ? -1 : left.name > right.name ? 1 : 0)
    for (const entry of entries) {
      const path = resolve(directory, entry.name)
      const artifactPath = relative(root, path).split(sep).join('/')
      if (artifactPath === 'manifest.json') continue
      const stats = lstatSync(path)
      if (stats.isDirectory()) {
        inventory[artifactPath] = { type: 'directory', mode: stats.mode & 0o7777 }
        visit(path)
      } else if (stats.isFile()) {
        inventory[artifactPath] = {
          type: 'file',
          mode: stats.mode & 0o7777,
          size: stats.size,
          sha256: sha256File(path),
        }
      } else if (stats.isSymbolicLink()) {
        const target = readlinkSync(path)
        assertContainedSymlink(root, path, target)
        inventory[artifactPath] = { type: 'symlink', target }
      } else {
        throw new Error(`Unsupported CEF artifact type: ${artifactPath}`)
      }
    }
  }
}

export function verifyCefArtifactInventory(root: string, expected: unknown): void {
  if (!isRecord(expected)) throw new Error('CEF artifact manifest has no complete artifact inventory')
  const actual = readCefArtifactInventory(root)
  const expectedPaths = Object.keys(expected).sort()
  const actualPaths = Object.keys(actual).sort()
  const missing = expectedPaths.filter((path) => !Object.prototype.hasOwnProperty.call(actual, path))
  const extra = actualPaths.filter((path) => !Object.prototype.hasOwnProperty.call(expected, path))
  if (missing.length > 0 || extra.length > 0) {
    throw new Error(`CEF artifact inventory differs (missing: ${missing.join(', ') || 'none'}; extra: ${extra.join(', ') || 'none'})`)
  }
  for (const path of expectedPaths) {
    const expectedEntry = expected[path]
    const actualEntry = actual[path]
    if (!validArtifactEntry(expectedEntry) || JSON.stringify(actualEntry) !== JSON.stringify(expectedEntry)) {
      throw new Error(`CEF artifact does not match its manifest: ${path}`)
    }
  }
}

export function sha256File(path: string): string {
  const result = Bun.spawnSync(['/usr/bin/shasum', '-a', '256', path])
  if (result.exitCode !== 0) throw new Error(`Could not hash ${path}`)
  return new TextDecoder().decode(result.stdout).trim().split(/\s+/u)[0] ?? ''
}

function assertContainedSymlink(root: string, path: string, target: string): void {
  const lexicalRelative = relative(resolve(root), resolve(dirname(path), target))
  let resolvedRelative: string
  try {
    resolvedRelative = relative(realpathSync(root), realpathSync(path))
  } catch {
    throw new Error(`CEF artifact symlink target is missing: ${relative(root, path)}`)
  }
  if (
    isAbsolute(target)
    || lexicalRelative === '..'
    || lexicalRelative.startsWith(`..${sep}`)
    || resolvedRelative === '..'
    || resolvedRelative.startsWith(`..${sep}`)
  ) {
    throw new Error(`CEF artifact symlink escapes its package root: ${relative(root, path)}`)
  }
}

function validArtifactEntry(value: unknown): value is CefArtifactEntry {
  if (!isRecord(value) || typeof value.type !== 'string') return false
  if (value.type === 'directory') return Object.keys(value).length === 2 && validMode(value.mode)
  if (value.type === 'symlink') return Object.keys(value).length === 2 && typeof value.target === 'string'
  return value.type === 'file'
    && validMode(value.mode)
    && Number.isSafeInteger(value.size)
    && Number(value.size) >= 0
    && typeof value.sha256 === 'string'
    && /^[a-f0-9]{64}$/u.test(value.sha256)
    && Object.keys(value).length === 4
}

function validMode(value: unknown): boolean {
  return Number.isSafeInteger(value) && Number(value) >= 0 && Number(value) <= 0o7777
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
}
