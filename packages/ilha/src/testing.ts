import {
  TestRenderer,
  hasNativeTestRenderer,
  type TestRendererOptions,
} from "@gpuix/native/testing"
import type { Component } from "ilha/renderer"

import { createRoot, type Root } from "./root.js"

export * from "@gpuix/native/testing"

export interface TestRoot {
  root: Root
  renderer: TestRenderer
  render(component: Component): void
  flushSync<Value>(fn: () => Value): Value
  unmount(): void
}

/**
 * Mount an ilha component against the GPU-backed test renderer, with no window.
 *
 * ```ts
 * const test = createTestRoot()
 * test.render(Counter)
 * test.renderer.getPaintedText()
 * ```
 */
export function createTestRoot(options: TestRendererOptions = {}): TestRoot {
  let root!: Root
  const renderer = new TestRenderer({
    ...options,
    dispatchEvent: (event) => root.dispatch(event),
  })
  root = createRoot(renderer)
  return {
    flushSync(fn) {
      const value = root.flushSync(fn)
      renderer.flush()
      return value
    },
    render(component) {
      root.render(component)
      renderer.flush()
    },
    renderer,
    root,
    unmount() {
      root.unmount()
      renderer.flush()
    },
  }
}

export { TestRenderer, hasNativeTestRenderer }
