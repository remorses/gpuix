/// Text properties inherit from an ancestor, the way CSS inherits them.
///
/// GPUI does this itself. A `div` pushes its text style onto a window stack,
/// and `window.text_style()` composes the whole stack, so a `<text>` with no
/// style of its own paints with the nearest ancestor declaration.
///
/// These tests pin that behaviour. It comes from the pinned zed fork rather
/// than from this repository, so a fork bump could remove it without any
/// change here. Each case asserts the strong form: declaring a property on the
/// ancestor paints byte-identically to declaring it on the text itself.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsDiffer, expectScreenshotsEqual, SHOTS_DIR } from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `inherit-${name}.png`)

// White background and black text, so the glyphs are visible before the case
// changes anything. Without this only `color` shows up, because the default
// text colour is invisible against the default background.
const BOX = {
  width: 300,
  height: 100,
  backgroundColor: "#ffffff",
  color: "#000000",
} as const

const INHERITED: Array<[string, Record<string, unknown>]> = [
  ["color", { color: "#ff0000" }],
  ["fontSize", { fontSize: 30 }],
  ["fontWeight", { fontWeight: "bold" }],
  ["fontFamily", { fontFamily: "Courier New" }],
  ["lineHeight", { lineHeight: 2.5 }],
  ["textAlign", { textAlign: "right" }],
]

describeNative("text inheritance", () => {
  for (const [name, declaration] of INHERITED) {
    it(`${name} on an ancestor paints the same as ${name} on the text`, () => {
      const onAncestor = createTestRoot()
      onAncestor.render(
        <div style={{ ...BOX, ...declaration }}>
          <text>Hello there world</text>
        </div>
      )
      onAncestor.renderer.captureScreenshot(shot(`${name}-ancestor`))
      onAncestor.unmount()

      const undeclared = createTestRoot()
      undeclared.render(
        <div style={BOX}>
          <text>Hello there world</text>
        </div>
      )
      undeclared.renderer.captureScreenshot(shot(`${name}-undeclared`))
      undeclared.unmount()

      const onText = createTestRoot()
      onText.render(
        <div style={BOX}>
          <text style={declaration}>Hello there world</text>
        </div>
      )
      onText.renderer.captureScreenshot(shot(`${name}-text`))

      // The declaration has to do something, or the comparison below is empty.
      expectScreenshotsDiffer(shot(`${name}-ancestor`), shot(`${name}-undeclared`))
      expectScreenshotsEqual(shot(`${name}-ancestor`), shot(`${name}-text`))
    })
  }

  it("takes the nearest declaration when two ancestors disagree", () => {
    const nested = createTestRoot()
    nested.render(
      <div style={{ ...BOX, color: "#ff0000" }}>
        <div style={{ color: "#0000ff" }}>
          <text>Hello there world</text>
        </div>
      </div>
    )
    nested.renderer.captureScreenshot(shot("nearest-nested"))
    nested.unmount()

    const direct = createTestRoot()
    direct.render(
      <div style={BOX}>
        <div style={{ color: "#0000ff" }}>
          <text>Hello there world</text>
        </div>
      </div>
    )
    direct.renderer.captureScreenshot(shot("nearest-direct"))

    expectScreenshotsEqual(shot("nearest-nested"), shot("nearest-direct"))
  })
})
