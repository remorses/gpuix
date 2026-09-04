import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'
import { sha256File, verifyCefArtifactInventory } from './cef-artifacts.ts'

const packageRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const temporary = mkdtempSync(join(tmpdir(), 'gpuix-native-pack-'))

try {
  const packed = Bun.spawnSync(['npm', 'pack', '--silent', '--pack-destination', temporary], {
    cwd: packageRoot,
    env: { ...process.env, CI: '1' },
    stdout: 'pipe',
    stderr: 'pipe',
  })
  if (packed.exitCode !== 0) {
    throw new Error(`npm pack failed: ${new TextDecoder().decode(packed.stderr).trim()}`)
  }
  const archiveName = new TextDecoder().decode(packed.stdout).trim().split(/\r?\n/u).at(-1)
  if (!archiveName) throw new Error('npm pack did not report an archive')
  const archive = join(temporary, archiveName)
  const untarred = Bun.spawnSync(['tar', '-xzf', archive, '-C', temporary])
  if (untarred.exitCode !== 0) throw new Error('Could not extract the packed @gpuix/native archive')

  const packedRoot = join(temporary, 'package')
  const cefRoot = join(packedRoot, 'cef')
  const manifest = JSON.parse(readFileSync(join(cefRoot, 'manifest.json'), 'utf8')) as {
    arch?: unknown
    artifacts?: unknown
    nativeAddon?: { path?: unknown; sha256?: unknown }
  }
  verifyCefArtifactInventory(cefRoot, manifest.artifacts)

  const addonPath = manifest.nativeAddon?.path
  const addonHash = manifest.nativeAddon?.sha256
  if (typeof addonPath !== 'string' || isAbsolute(addonPath) || typeof addonHash !== 'string') {
    throw new Error('Packed CEF manifest has invalid native addon metadata')
  }
  const addonRelative = relative(packedRoot, resolve(packedRoot, addonPath))
  if (addonRelative === '..' || addonRelative.startsWith(`..${sep}`)) {
    throw new Error(`Packed CEF native addon escapes its package: ${addonPath}`)
  }

  let addon = resolve(packedRoot, addonPath)
  if (!existsSync(addon)) {
    if (manifest.arch !== 'arm64' && manifest.arch !== 'x64') {
      throw new Error(`Packed CEF manifest has an unsupported architecture: ${String(manifest.arch)}`)
    }
    const platformSource = join(packageRoot, 'npm', `darwin-${manifest.arch}`)
    if (!existsSync(platformSource)) {
      throw new Error(`Darwin platform package was not assembled: ${platformSource}`)
    }
    const platformArchive = Bun.spawnSync(['npm', 'pack', '--silent', '--pack-destination', temporary], {
      cwd: platformSource,
      env: { ...process.env, CI: '1' },
      stdout: 'pipe',
      stderr: 'pipe',
    })
    if (platformArchive.exitCode !== 0) {
      throw new Error(`npm pack failed for Darwin platform package: ${new TextDecoder().decode(platformArchive.stderr).trim()}`)
    }
    const platformArchiveName = new TextDecoder().decode(platformArchive.stdout).trim().split(/\r?\n/u).at(-1)
    if (!platformArchiveName) throw new Error('npm pack did not report the Darwin platform archive')
    const platformExtracted = join(temporary, 'platform')
    mkdirSync(platformExtracted)
    const platformUntarred = Bun.spawnSync(['tar', '-xzf', join(temporary, platformArchiveName), '-C', platformExtracted])
    if (platformUntarred.exitCode !== 0) throw new Error('Could not extract the packed Darwin platform archive')
    addon = resolve(platformExtracted, 'package', addonPath)
  }

  if (!existsSync(addon)) throw new Error(`Packed CEF native addon is missing: ${addonPath}`)
  if (sha256File(addon) !== addonHash) throw new Error(`Packed CEF native addon hash differs: ${addonPath}`)

  console.log(`[gpuix] verified packed Chromium runtime in ${archiveName}`)
} finally {
  rmSync(temporary, { recursive: true, force: true })
}
