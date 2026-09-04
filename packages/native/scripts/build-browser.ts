import { cpSync, existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, readlinkSync, realpathSync, rmSync, writeFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path'
import { fileURLToPath } from 'node:url'

if (process.platform !== 'darwin') {
  throw new Error('GPUix embedded Chromium is currently supported on macOS only')
}

const CEF_VERSION = '151.3.24'
const packageDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..')
const cefCache = resolve(process.env.CEF_PATH ?? join(homedir(), 'Library', 'Caches', 'gpuix', 'cef'))
const runtimeShaders = process.env.GPUIX_RUNTIME_SHADERS === '1'
const features = ['test-support', 'native-browser-cef', ...(runtimeShaders ? ['runtime-shaders'] : [])].join(',')
const environment = { ...process.env, CEF_PATH: cefCache, CARGO_NET_GIT_FETCH_WITH_CLI: 'true' }
const target = parseTarget(process.argv.slice(2))
const architecture = target?.startsWith('aarch64-') ? 'arm64'
  : target?.startsWith('x86_64-') ? 'x64'
    : process.arch
const targetArguments = target ? ['--target', target] : []

run('bunx', ['napi', 'build', '--platform', '--release', '--features', features, ...targetArguments], environment)
run('cargo', ['build', '--release', '--bin', 'gpuix-cef-helper', '--features', features, ...targetArguments], environment)

const cefDirectory = locateCefDirectory(cefCache, architecture)
const cefApiVersion = validateCefVersion(cefDirectory)
const destination = join(packageDirectory, 'cef')
rmSync(destination, { recursive: true, force: true })
mkdirSync(destination, { recursive: true })
cpSync(
  join(cefDirectory, 'Chromium Embedded Framework.framework'),
  join(destination, 'Chromium Embedded Framework.framework'),
  { recursive: true },
)
if (existsSync(join(cefDirectory, 'CREDITS.html'))) {
  cpSync(join(cefDirectory, 'CREDITS.html'), join(destination, 'CREDITS.html'))
}

const helperBaseName = 'GPUix Chromium Helper'
const helperVariants = [
  { nameSuffix: '', identifierSuffix: '' },
  { nameSuffix: ' (Alerts)', identifierSuffix: '.alerts' },
  { nameSuffix: ' (GPU)', identifierSuffix: '.gpu' },
  { nameSuffix: ' (Plugin)', identifierSuffix: '.plugin' },
  { nameSuffix: ' (Renderer)', identifierSuffix: '.renderer' },
]
for (const variant of helperVariants) {
  const helperName = `${helperBaseName}${variant.nameSuffix}`
  const helperBundle = join(destination, `${helperName}.app`)
  const helperContents = join(helperBundle, 'Contents')
  const helperMacOS = join(helperContents, 'MacOS')
  mkdirSync(helperMacOS, { recursive: true })
  mkdirSync(join(helperContents, 'Resources'), { recursive: true })
  const helperTarget = target
    ? join(packageDirectory, 'target', target, 'release', 'gpuix-cef-helper')
    : join(packageDirectory, 'target', 'release', 'gpuix-cef-helper')
  cpSync(helperTarget, join(helperMacOS, helperName))
  writeFileSync(join(helperContents, 'Info.plist'), helperPlist(helperName, variant.identifierSuffix))
  writeFileSync(join(helperContents, 'PkgInfo'), 'APPL????')

  if (process.platform === 'darwin') {
    run('codesign', ['--force', '--sign', '-', '--timestamp=none', helperBundle], environment)
  }
}

const nativeAddon = join(packageDirectory, `gpuix-native.darwin-${architecture}.node`)
if (!existsSync(nativeAddon)) throw new Error(`CEF-enabled native addon is missing: ${nativeAddon}`)
const frameworkBinary = join(destination, 'Chromium Embedded Framework.framework', 'Chromium Embedded Framework')
for (const executable of [nativeAddon, frameworkBinary, ...helperVariants.map((variant) => {
  const helperName = `${helperBaseName}${variant.nameSuffix}`
  return join(destination, `${helperName}.app`, 'Contents', 'MacOS', helperName)
})]) assertArchitecture(executable)
writeFileSync(join(destination, 'manifest.json'), `${JSON.stringify({
  schemaVersion: 2,
  cefVersion: CEF_VERSION,
  cefApiVersion,
  platform: 'darwin',
  arch: architecture,
  minMacOS: '13.0',
  nativeAddon: { path: nativeAddon.slice(packageDirectory.length + 1), sha256: sha256(nativeAddon) },
  artifacts: artifactInventory(destination),
}, null, 2)}\n`)
console.log(`[gpuix] staged Chromium ${cefDirectory} -> ${destination}`)

type ArtifactEntry =
  | { type: 'directory'; mode: number }
  | { type: 'file'; mode: number; size: number; sha256: string }
  | { type: 'symlink'; target: string }

function artifactInventory(root: string): Record<string, ArtifactEntry> {
  const inventory: Record<string, ArtifactEntry> = {}
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
      if (entry.isDirectory()) {
        inventory[artifactPath] = { type: 'directory', mode: stats.mode & 0o777 }
        visit(path)
      } else if (entry.isFile()) {
        inventory[artifactPath] = {
          type: 'file',
          mode: stats.mode & 0o777,
          size: stats.size,
          sha256: sha256(path),
        }
      } else if (entry.isSymbolicLink()) {
        const target = readlinkSync(path)
        assertContainedSymlink(root, path, target)
        inventory[artifactPath] = { type: 'symlink', target }
      } else {
        throw new Error(`Unsupported CEF artifact type: ${artifactPath}`)
      }
    }
  }
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

function parseTarget(arguments_: string[]): string | undefined {
  if (arguments_.length === 0) return undefined
  if (arguments_.length === 2 && arguments_[0] === '--target' && arguments_[1]) {
    return arguments_[1]
  }
  throw new Error('Usage: bun run build:browser [--target <rust-target>]')
}

function run(command: string, arguments_: string[], env: Record<string, string | undefined>): void {
  const result = Bun.spawnSync([command, ...arguments_], {
    cwd: packageDirectory,
    env,
    stdin: 'inherit',
    stdout: 'inherit',
    stderr: 'inherit',
  })
  if (result.exitCode !== 0) process.exit(result.exitCode)
}

function locateCefDirectory(cache: string, nodeArchitecture: string): string {
  const architecture = nodeArchitecture === 'arm64' ? 'aarch64' : nodeArchitecture === 'x64' ? 'x86_64' : nodeArchitecture
  const candidates = [
    join(cache, CEF_VERSION, `cef_macos_${architecture}`),
    cache,
  ]
  const found = candidates.find((candidate) =>
    existsSync(join(candidate, 'Chromium Embedded Framework.framework', 'Chromium Embedded Framework')),
  )
  if (!found) throw new Error(`CEF ${CEF_VERSION} was not found beneath ${cache}`)
  return found
}

function validateCefVersion(directory: string): number {
  const versionHeader = readFileSync(join(directory, 'include', 'cef_version.h'), 'utf8')
  const runtimeVersion = versionHeader.match(/^#define CEF_VERSION "([^"]+)"$/mu)?.[1]
  if (!runtimeVersion?.startsWith(`${CEF_VERSION}+`)) {
    throw new Error(`CEF_PATH contains ${runtimeVersion ?? 'an unknown version'}, expected ${CEF_VERSION}`)
  }
  const apiHeader = readFileSync(join(directory, 'include', 'cef_api_versions.h'), 'utf8')
  const alias = apiHeader.match(/^#define CEF_API_VERSION_LAST CEF_API_VERSION_(\d+)$/mu)?.[1]
  if (!alias) throw new Error('Could not read CEF_API_VERSION_LAST')
  return Number(alias)
}

function sha256(path: string): string {
  const result = Bun.spawnSync(['/usr/bin/shasum', '-a', '256', path])
  if (result.exitCode !== 0) throw new Error(`Could not hash ${path}`)
  return new TextDecoder().decode(result.stdout).trim().split(/\s+/u)[0] ?? ''
}

function assertArchitecture(path: string): void {
  const result = Bun.spawnSync(['/usr/bin/lipo', '-archs', path])
  const architectures = new TextDecoder().decode(result.stdout).trim().split(/\s+/u)
  const expected = architecture === 'x64' ? 'x86_64' : architecture
  if (result.exitCode !== 0 || !architectures.includes(expected)) {
    throw new Error(`${path} does not contain the required ${expected} architecture`)
  }
}

function helperPlist(executable: string, identifierSuffix: string): string {
  return `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key><string>en</string>
  <key>CFBundleDisplayName</key><string>${executable}</string>
  <key>CFBundleExecutable</key><string>${executable}</string>
  <key>CFBundleIdentifier</key><string>dev.gpuix.chromium.helper${identifierSuffix}</string>
  <key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
  <key>CFBundleName</key><string>${executable}</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.7.0</string>
  <key>CFBundleSignature</key><string>????</string>
  <key>CFBundleVersion</key><string>0.7.0</string>
  <key>LSEnvironment</key><dict><key>MallocNanoZone</key><string>0</string></dict>
  <key>LSFileQuarantineEnabled</key><true/>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>LSUIElement</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
`
}
