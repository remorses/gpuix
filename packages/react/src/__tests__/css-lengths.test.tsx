/// `lineHeight` the way CSS reads it, and `calc()` in a length.
///
/// Both cases paint twice and compare. A JS number is pixels, as in React
/// Native, so `lineHeight: 40` and `"40px"` land on the same pixels. A string
/// follows CSS, so `"2.5"` and `"250%"` at a 16 px font land on 40 pixels. A
/// `calc()` has to land on the same pixels as the number it folds to.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import {
  expectScreenshotsDiffer,
  expectScreenshotsEqual,
  SHOTS_DIR,
} from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `css-length-${name}.png`)

// White on black, or the glyphs are invisible and every line height paints
// the same nothing.
const TEXT_BOX = {
  width: 200,
  height: 140,
  backgroundColor: "#ffffff",
  color: "#000000",
} as const

const BOX = { width: 200, height: 140 } as const

function paint(name: string, tree: React.ReactElement) {
  const root = createTestRoot()
  root.render(tree)
  root.renderer.captureScreenshot(shot(name))
  root.unmount()
}

/// Text that wraps, so the space between two lines is on screen.
const lines = (declaration: Record<string, unknown>) => (
  <div style={{ ...TEXT_BOX, fontSize: 16, ...declaration }}>
    <text>one two three four five six seven eight nine ten</text>
  </div>
)

describeNative("line height", () => {
  it("reads a bare number in a string as a multiple of the font size", () => {
    // "2.5" used to mean 2.5 pixels. In CSS it means 40 pixels at a 16 px font.
    paint("multiple", lines({ lineHeight: "2.5" }))
    paint("pixels", lines({ lineHeight: "40px" }))
    paint("unset", lines({}))
    // The declaration has to do something, or the comparison below is empty.
    expectScreenshotsDiffer(shot("multiple"), shot("unset"))
    expectScreenshotsEqual(shot("multiple"), shot("pixels"))
  })

  it("reads a percentage as the same multiple", () => {
    paint("percent", lines({ lineHeight: "250%" }))
    paint("percent-direct", lines({ lineHeight: "40px" }))
    expectScreenshotsEqual(shot("percent"), shot("percent-direct"))
  })

  it("reads a JS number as pixels", () => {
    // React Native reads `lineHeight: 40` as 40 pixels, and so does GPUIX.
    paint("number-pixels", lines({ lineHeight: 40 }))
    paint("number-pixels-direct", lines({ lineHeight: "40px" }))
    expectScreenshotsEqual(shot("number-pixels"), shot("number-pixels-direct"))
  })

  it("reads rem against the root font size", () => {
    paint("rem", lines({ lineHeight: "2.5rem" }))
    paint("rem-direct", lines({ lineHeight: "40px" }))
    expectScreenshotsEqual(shot("rem"), shot("rem-direct"))
  })

  it("declares nothing for a line height of zero", () => {
    paint("zero", lines({ lineHeight: 0 }))
    paint("zero-unset", lines({}))
    expectScreenshotsEqual(shot("zero"), shot("zero-unset"))
  })
})

describeNative("calc", () => {
  it("folds arithmetic to the same length as the number", () => {
    paint("calc-sum", <div style={{ ...BOX, padding: "calc(8px + 12px)", backgroundColor: "#ff0000" }}>
      <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
    </div>)
    paint("calc-sum-direct", <div style={{ ...BOX, padding: 20, backgroundColor: "#ff0000" }}>
      <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
    </div>)
    expectScreenshotsEqual(shot("calc-sum"), shot("calc-sum-direct"))
  })

  it("adds a rem to a pixel length", () => {
    paint("calc-rem", <div style={{ ...BOX, borderWidth: "calc(1rem + 4px)", borderColor: "#ff0000" }} />)
    paint("calc-rem-direct", <div style={{ ...BOX, borderWidth: 20, borderColor: "#ff0000" }} />)
    expectScreenshotsEqual(shot("calc-rem"), shot("calc-rem-direct"))
  })

  it("folds a variable inside the arithmetic", () => {
    // This is the shape every step of the Tailwind spacing scale takes.
    paint("calc-var", <div style={{ ...BOX, "--spacing": "0.25rem", padding: "calc(var(--spacing) * 6)", backgroundColor: "#ff0000" }}>
      <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
    </div>)
    paint("calc-var-direct", <div style={{ ...BOX, padding: 24, backgroundColor: "#ff0000" }}>
      <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
    </div>)
    expectScreenshotsEqual(shot("calc-var"), shot("calc-var-direct"))
  })

  it("takes min, max and clamp", () => {
    paint("clamp", <div style={{ ...BOX, borderWidth: "clamp(4px, 1rem, 10px)", borderColor: "#ff0000" }} />)
    paint("clamp-direct", <div style={{ ...BOX, borderWidth: 10, borderColor: "#ff0000" }} />)
    expectScreenshotsEqual(shot("clamp"), shot("clamp-direct"))
  })
})
