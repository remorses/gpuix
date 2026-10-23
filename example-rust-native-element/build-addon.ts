// Vitest global setup: builds the crate and copies the library to
// rust-greeting.node. Plain cargo instead of `napi build`, because napi-rs
// forces a rebuild on every run, and the LTO link alone takes over a minute.
import { execFileSync } from 'node:child_process'
import { copyFileSync } from 'node:fs'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'

const HERE = fileURLToPath(new URL('.', import.meta.url))
export const ADDON_PATH = join(HERE, 'rust-greeting.node')

const LIBRARY_FILE: Partial<Record<NodeJS.Platform, string>> = {
  darwin: 'libexample_rust_native_element.dylib',
  linux: 'libexample_rust_native_element.so',
  win32: 'example_rust_native_element.dll',
}

export default function setup() {
  const library = LIBRARY_FILE[process.platform]
  if (!library) throw new Error(`No addon file name for ${process.platform}`)

  console.log('building example-rust-native-element (cargo build --release)')
  execFileSync('cargo', ['build', '--release'], { cwd: HERE, stdio: 'inherit' })

  const metadata = JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps'], {
      cwd: HERE,
      encoding: 'utf8',
    }),
  ) as { target_directory: string }
  copyFileSync(join(metadata.target_directory, 'release', library), ADDON_PATH)
  console.log(`copied addon to ${ADDON_PATH}`)
}
