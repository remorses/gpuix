/**
 * Compile one example into a standalone Bun binary.
 *
 * The binary holds the script, the Bun runtime and the native `.node`, so it
 * runs on a machine with nothing installed. On macOS it is wrapped in a `.app`
 * so Finder and the Dock can show it.
 *
 *   bun compile.ts chat.tsx --name "GPUIX Chat" --id dev.gpuix.chat --icon assets/icons/openai-mark.svg --tint "#10a37f"
 *   bun compile.ts demo.tsx --name "GPUIX Demo" --id dev.gpuix.demo
 *
 * The icon needs `rsvg-convert` (librsvg) and, on Windows, `magick`. When
 * either is missing the icon is skipped and the app takes the system default.
 *
 * CI sets COMPILE_OUT, COMPILE_TARGET, COMPILE_SKIP_ICONS, COMPILE_SKIP_APP.
 */
import { spawnSync } from 'node:child_process'
import { existsSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import path from 'node:path'
import { parseArgs } from 'node:util'
import { fileURLToPath } from 'node:url'

const ROOT = path.dirname(fileURLToPath(import.meta.url))

const { values, positionals } = parseArgs({
  allowPositionals: true,
  options: {
    name: { type: 'string' },
    id: { type: 'string' },
    icon: { type: 'string' },
    tint: { type: 'string', default: '#10a37f' },
  },
})

const entry = positionals[0]
if (!entry || !values.name || !values.id) {
  console.error('usage: bun compile.ts <entry.tsx> --name <app name> --id <bundle id> [--icon <svg>] [--tint <hex>]')
  process.exit(2)
}

const ENTRY = path.resolve(ROOT, entry)
const SLUG = path.basename(entry, path.extname(entry))
const APP_NAME = values.name
const BUNDLE_ID = values.id
const DIST = path.join(ROOT, 'dist', SLUG)
const PNG = path.join(DIST, 'app-icon.png')
const ICO = path.join(DIST, 'app-icon.ico')
const ICNS = path.join(DIST, 'app-icon.icns')
const COMPILE_TARGET = process.env.COMPILE_TARGET
const WINDOWS = process.platform === 'win32' || (COMPILE_TARGET ?? '').includes('windows')
const BINARY = path.join(DIST, outputName())
const APP_BUNDLE = path.join(DIST, `${APP_NAME}.app`)

function outputName(): string {
  const requested = process.env.COMPILE_OUT
  if (requested) {
    return WINDOWS && !requested.endsWith('.exe') ? `${requested}.exe` : requested
  }
  return WINDOWS ? `${SLUG}.exe` : SLUG
}

function log(message: string): void {
  console.log(`[compile] ${message}`)
}

function run(command: string, args: string[]): void {
  log(`run: ${command} ${args.join(' ')}`)
  const result = spawnSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] })
  const stdout = result.stdout.trim()
  const stderr = result.stderr.trim()
  if (stdout && command !== 'sips') console.log(stdout)
  if (stderr && command !== 'sips') console.error(stderr)
  if (result.status !== 0) {
    throw new Error(`${command} failed with exit ${result.status}`)
  }
}

/// Whether the icon can be built here. Returns the reason when it cannot.
function iconBlocker(): string | null {
  if (process.env.COMPILE_SKIP_ICONS === '1') return 'COMPILE_SKIP_ICONS is set'
  if (!values.icon) return 'no --icon given'
  if (!Bun.which('rsvg-convert')) return 'rsvg-convert is not installed'
  if (process.platform === 'win32' && !Bun.which('magick')) return 'magick is not installed'
  return null
}

async function buildIcons(): Promise<boolean> {
  const blocker = iconBlocker()
  if (blocker) {
    log(`skipping the icon: ${blocker}`)
    return false
  }
  const source = path.resolve(ROOT, values.icon!)
  log(`building icons from ${path.relative(ROOT, source)}`)
  const svg = (await Bun.file(source).text()).replace('fill="currentColor"', 'fill="#ffffff"')
  const whiteSvg = path.join(DIST, 'app-icon.svg')
  await Bun.write(whiteSvg, svg)

  run('rsvg-convert', ['-w', '1024', '-h', '1024', '--background-color', values.tint!, whiteSvg, '-o', PNG])
  log(`wrote ${path.relative(ROOT, PNG)}`)

  if (process.platform === 'win32') {
    run('magick', [PNG, '-define', 'icon:auto-resize=256,128,64,48,32,16', ICO])
    log(`wrote ${path.relative(ROOT, ICO)}`)
  }

  if (process.platform !== 'darwin') return true

  const iconset = path.join(DIST, 'app-icon.iconset')
  rmSync(iconset, { recursive: true, force: true })
  mkdirSync(iconset, { recursive: true })
  const sizes = [
    [16, 'icon_16x16.png'],
    [32, 'icon_16x16@2x.png'],
    [32, 'icon_32x32.png'],
    [64, 'icon_32x32@2x.png'],
    [128, 'icon_128x128.png'],
    [256, 'icon_128x128@2x.png'],
    [256, 'icon_256x256.png'],
    [512, 'icon_256x256@2x.png'],
    [512, 'icon_512x512.png'],
    [1024, 'icon_512x512@2x.png'],
  ] as const
  for (const [px, name] of sizes) {
    run('sips', ['-z', String(px), String(px), PNG, '--out', path.join(iconset, name)])
  }
  run('iconutil', ['-c', 'icns', iconset, '-o', ICNS])
  log(`wrote ${path.relative(ROOT, ICNS)}`)
  return true
}

async function compileBinary(withIcon: boolean): Promise<void> {
  log(`bundling ${path.relative(ROOT, ENTRY)} into a standalone binary`)
  const compile: {
    outfile: string
    target?: string
    windows?: {
      icon?: string
      hideConsole: boolean
      title: string
      publisher: string
      version: string
      description: string
    }
  } = { outfile: BINARY }
  if (COMPILE_TARGET) {
    compile.target = COMPILE_TARGET
    log(`target ${COMPILE_TARGET}`)
  }
  if (WINDOWS) {
    compile.windows = {
      hideConsole: true,
      title: APP_NAME,
      publisher: 'GPUIX',
      version: '0.1.0',
      description: `${APP_NAME}, a desktop app built with GPUIX`,
    }
    // Only set the key when the file exists. Bun rejects `icon: undefined`
    // with "windows.icon must be a valid path to an ico file".
    if (withIcon && process.platform === 'win32' && existsSync(ICO)) {
      compile.windows.icon = ICO
    }
  }

  const result = await Bun.build({ entrypoints: [ENTRY], compile, minify: true })
  if (!result.success) {
    for (const message of result.logs) console.error(message)
    throw new Error('bun build --compile failed')
  }
  const output = result.outputs[0]?.path ?? BINARY
  log(`wrote ${path.relative(ROOT, output)}`)
}

function wrapMacApp(withIcon: boolean): void {
  if (process.env.COMPILE_SKIP_APP === '1') return
  if (process.platform !== 'darwin') return
  if (COMPILE_TARGET && !COMPILE_TARGET.includes('darwin')) return
  log(`wrapping ${path.relative(ROOT, BINARY)} in ${path.basename(APP_BUNDLE)}`)
  rmSync(APP_BUNDLE, { recursive: true, force: true })
  const macos = path.join(APP_BUNDLE, 'Contents', 'MacOS')
  const resources = path.join(APP_BUNDLE, 'Contents', 'Resources')
  mkdirSync(macos, { recursive: true })
  mkdirSync(resources, { recursive: true })

  const executable = path.join(macos, SLUG)
  run('cp', [BINARY, executable])
  run('chmod', ['+x', executable])
  if (withIcon) run('cp', [ICNS, path.join(resources, 'AppIcon.icns')])

  const plist = [
    '<?xml version="1.0" encoding="UTF-8"?>',
    '<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">',
    '<plist version="1.0">',
    '<dict>',
    '  <key>CFBundleDevelopmentRegion</key>',
    '  <string>en</string>',
    '  <key>CFBundleDisplayName</key>',
    `  <string>${APP_NAME}</string>`,
    '  <key>CFBundleExecutable</key>',
    `  <string>${SLUG}</string>`,
    ...(withIcon ? ['  <key>CFBundleIconFile</key>', '  <string>AppIcon</string>'] : []),
    '  <key>CFBundleIdentifier</key>',
    `  <string>${BUNDLE_ID}</string>`,
    '  <key>CFBundleInfoDictionaryVersion</key>',
    '  <string>6.0</string>',
    '  <key>CFBundleName</key>',
    `  <string>${APP_NAME}</string>`,
    '  <key>CFBundlePackageType</key>',
    '  <string>APPL</string>',
    '  <key>CFBundleShortVersionString</key>',
    '  <string>0.1.0</string>',
    '  <key>CFBundleVersion</key>',
    '  <string>1</string>',
    '  <key>LSMinimumSystemVersion</key>',
    '  <string>13.0</string>',
    '  <key>NSHighResolutionCapable</key>',
    '  <true/>',
    '</dict>',
    '</plist>',
    '',
  ].join('\n')
  writeFileSync(path.join(APP_BUNDLE, 'Contents', 'Info.plist'), plist)
  // The binary Bun writes carries only the linker's ad hoc signature, which
  // does not cover the payload Bun appends. LaunchServices kills it with
  // "Code Signature Invalid" when the app opens from Finder, while a terminal
  // lets it run. Signing again covers the whole file.
  run('codesign', ['--force', '--sign', '-', executable])
  run('codesign', ['--force', '--sign', '-', APP_BUNDLE])
  run('touch', [APP_BUNDLE])
  log(`wrote ${path.relative(ROOT, APP_BUNDLE)}`)
}

async function main(): Promise<void> {
  log(`output dir ${path.relative(ROOT, DIST)}`)
  rmSync(DIST, { recursive: true, force: true })
  mkdirSync(DIST, { recursive: true })
  const withIcon = await buildIcons()
  await compileBinary(withIcon)
  wrapMacApp(withIcon)
  log('done')
  if (process.platform === 'darwin' && existsSync(APP_BUNDLE)) {
    log(`run: open "${APP_BUNDLE}"`)
  } else {
    log(`run: ${BINARY}`)
  }
}

await main()
