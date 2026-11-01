/// Clipboard: `writeToClipboard` and `readFromClipboard` round-trip through
/// the native binding.
///
/// `VisualTestPlatform` keeps the test renderer's clipboard in memory, so these
/// tests never read or replace the developer's system clipboard.
///
/// Regression guarded: reading used to lose an empty string, because
/// `ClipboardItem::text` reports `None` for empty text. Writing `""` must
/// leave an empty entry behind instead of the previous clipboard contents.

import React from "react"
import { afterEach, describe, expect, it } from "vitest"
import { GpuixRenderer } from "@gpuix/native"
import { render, resetRender } from "../reconciler/renderer.js"
import { hasNativeTestRenderer, TestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

function mountClipboardWindow(): TestRenderer {
  resetRender()
  const renderer = new TestRenderer()
  render(<div style={{ width: 40, height: 40 }} />, { renderer })
  return renderer
}

describeNative("clipboard", () => {
  afterEach(() => resetRender())

  it("reads back the text it wrote", () => {
    const renderer = mountClipboardWindow()
    renderer.writeToClipboard("gpuix clipboard — é世")
    expect(renderer.readFromClipboard()).toBe("gpuix clipboard — é世")
  })

  it("writes an empty string as an entry instead of no-op", () => {
    const renderer = mountClipboardWindow()
    renderer.writeToClipboard("previous contents")
    renderer.writeToClipboard("")
    expect(renderer.readFromClipboard()).toBe("")
  })

  it("reports a clipboard with no text as null", () => {
    const renderer = mountClipboardWindow()
    expect(renderer.readFromClipboard()).toBeNull()
  })

  it("fails to read through a renderer that never opened a window", () => {
    const renderer = new GpuixRenderer()
    expect(() => renderer.readFromClipboard()).toThrow(/not initialized|not running/)
  })
})
