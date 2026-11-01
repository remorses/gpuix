import { defineConfig } from 'vitest/config'
import { ADDON_PATH } from './build-addon.ts'

export default defineConfig({
  test: {
    globalSetup: ['./build-addon.ts'],
    // Load this crate's addon in place of the prebuilt @gpuix/native binary.
    env: { NAPI_RS_NATIVE_LIBRARY_PATH: ADDON_PATH },
    // The first build compiles GPUI unless packages/native already did.
    hookTimeout: 30 * 60_000,
  },
})
