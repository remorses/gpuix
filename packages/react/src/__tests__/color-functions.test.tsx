import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import {
  expectScreenshotsDiffer,
  expectScreenshotsEqual,
  SHOTS_DIR,
} from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

// Every case here is a colour some CSS specification defines. `hsv()`,
// `hsva()`, `hwba()` and bare hex with no `#` used to sit in these lists.
// No CSS specification defines any of them. They came from csscolorparser,
// which GPUIX no longer uses.
const absoluteCases = [
  ["hex4", "#f00f", "#ff0000"],
  ["named", "rebeccapurple", "#663399"],
  ["rgb", "rgb(255 0 0)", "#ff0000"],
  ["rgba", "rgba(255, 0, 0, 1)", "#ff0000"],
  ["hsl", "hsl(0 100% 50%)", "#ff0000"],
  ["hsla", "hsla(0, 100%, 50%, 1)", "#ff0000"],
  ["hwb", "hwb(0 0% 0%)", "#ff0000"],
  ["lab", "lab(100% 0 0)", "#ffffff"],
  ["lch", "lch(100% 0 0)", "#ffffff"],
  ["oklab", "oklab(0 0 0)", "#000000"],
  ["oklch", "oklch(0 0 0)", "#000000"],
  // Both of these came back as invalid before the move to lightningcss.
  ["color-mix", "color-mix(in srgb, #ff0000 100%, #0000ff 0%)", "#ff0000"],
  ["light-dark", "light-dark(#ff0000, #ff0000)", "#ff0000"],
] as const

const alphaCases = [
  ["rgb", "rgb(0 0 0 / 50%)"],
  ["rgba", "rgba(0, 0, 0, 0.5)"],
  ["hsl", "hsl(0 0% 0% / 50%)"],
  ["hsla", "hsla(0, 0%, 0%, 0.5)"],
  ["hwb", "hwb(0 0% 100% / 50%)"],
  ["lab", "lab(0% 0 0 / 50%)"],
  ["lch", "lch(0% 0 0 / 50%)"],
  ["oklab", "oklab(0 0 0 / 50%)"],
  ["oklch", "oklch(0 0 0 / 50%)"],
] as const

const relativeCases = [
  ["rgb", "rgb(from #bad455 b r g / alpha)", "#55bad4"],
  ["hsl", "hsl(from #bad455 h s l / alpha)", "#bad455"],
  ["hwb", "hwb(from #bad455 h w b / alpha)", "#bad455"],
  ["lab", "lab(from #bad455 l a b / alpha)", "#bad455"],
  ["lch", "lch(from #bad455 l c h / alpha)", "#bad455"],
  ["oklab", "oklab(from #bad455 calc(l * 0.7) a b)", "#708500"],
  ["oklch", "oklch(from #bad455 calc(l - 0.15) calc(c * 0.7) h)", "#8fa150"],
] as const

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

function captureColor(name: string, color?: string) {
  const screenshotPath = path.join(SHOTS_DIR, `${name}.png`)
  const testRoot = createTestRoot()
  testRoot.render(
    <div
      style={{
        width: "100%",
        height: "100%",
        backgroundColor: color,
      }}
    />
  )
  testRoot.renderer.captureScreenshot(screenshotPath)
  return screenshotPath
}

function expectColorsEqual(name: string, input: string, expected: string) {
  const actualPath = captureColor(`${name}-actual`, input)
  const expectedPath = captureColor(`${name}-expected`, expected)
  expectScreenshotsEqual(actualPath, expectedPath)
}

/// Paint `color` over a white parent and read one pixel. The window itself is
/// black, and translucent black over black is black, so alpha only shows over
/// an explicit light background.
function paintedPixel(color: string) {
  const testRoot = createTestRoot()
  testRoot.render(
    <div style={{ width: "100%", height: "100%", backgroundColor: "#ffffff" }}>
      <div style={{ width: "100%", height: "100%", backgroundColor: color }} />
    </div>
  )
  return testRoot.renderer.pixelAt(2, 2)
}

/// lightningcss keeps the alpha of `rgb()`, `hsl()` and `hwb()` in 8 bits, so
/// 50% comes back as 128/255. The wider spaces keep the exact float 0.5. Over
/// white, the first paints 127 and the second 128, and which side of the
/// boundary the GPU takes differs between Metal and Direct3D. So this allows
/// one 8-bit step, the same tolerance the engine colour tests use.
function expectColorsClose(input: string, expected: string) {
  const actual = paintedPixel(input)
  const reference = paintedPixel(expected)
  for (let channel = 0; channel < 4; channel++) {
    expect(Math.abs(actual[channel]! - reference[channel]!)).toBeLessThanOrEqual(1)
  }
}

describeNative("native color functions", () => {
  it.each(absoluteCases)(
    "paints absolute %s exactly like its canonical hex",
    (name, input, expected) => {
      expectColorsEqual(`color-absolute-${name}`, input, expected)
    }
  )

  it.each(alphaCases)(
    "paints %s alpha like 50% black within one 8-bit step",
    (_name, input) => {
      expectColorsClose(input, "rgba(0 0 0 / 50%)")
    }
  )

  it.each(relativeCases)(
    "paints relative %s exactly like its expected hex",
    (name, input, expected) => {
      expectColorsEqual(`color-relative-${name}`, input, expected)
    }
  )

  it("ignores an invalid paint and paints a valid OKLCH value", () => {
    const invalidPath = captureColor("color-invalid", "not-a-color")
    const unsetPath = captureColor("color-unset")
    expectScreenshotsEqual(invalidPath, unsetPath)

    const validPath = captureColor("color-valid-oklch", "oklch(67.3% 0.182 276.935)")
    expectScreenshotsDiffer(validPath, unsetPath)
  })

  it.each([
    ["hex with no hash", "ff0000ff"],
    ["hwba", "hwba(0, 0%, 0%, 1)"],
    ["hsv", "hsv(0 100% 100%)"],
    ["hsva", "hsva(0, 100%, 100%, 1)"],
  ])("ignores %s, which no CSS specification defines", (_name, input) => {
    const paintedPath = captureColor("color-nonstandard-actual", input)
    const unsetPath = captureColor("color-nonstandard-unset")
    expectScreenshotsEqual(paintedPath, unsetPath)
  })

  it("uses the same parser for compound consumers and pseudo-states", () => {
    const basePath = path.join(SHOTS_DIR, "color-consumers-base.png")
    const hoverPath = path.join(SHOTS_DIR, "color-consumers-hover.png")
    const activePath = path.join(SHOTS_DIR, "color-consumers-active.png")
    const testRoot = createTestRoot()

    testRoot.render(
      <div
        style={{
          width: 360,
          height: 180,
          backgroundColor: "oklch(67.3% 0.182 276.935)",
          borderWidth: 8,
          borderColor: "hwb(0 0% 0%)",
          color: "hsl(0 0% 100%)",
          selectionColor: "lab(70% 40 30 / 35%)",
          boxShadow: {
            offsetX: 18,
            offsetY: 18,
            blurRadius: 10,
            spreadRadius: 4,
            color: "oklab(45% 0.1 0.05 / 45%)",
          },
          hover: { backgroundColor: "hwb(210 20% 30%)" },
          active: { backgroundColor: "lch(60% 80 40)" },
        }}
      >
        <text>Full color path</text>
      </div>
    )

    testRoot.renderer.nativeSimulateMouseMove(500, 500)
    testRoot.renderer.captureScreenshot(basePath)
    testRoot.renderer.nativeSimulateMouseMove(180, 90)
    testRoot.renderer.captureScreenshot(hoverPath)
    testRoot.renderer.nativeSimulateMouseDown(180, 90)
    testRoot.renderer.captureScreenshot(activePath)

    expectScreenshotsDiffer(basePath, hoverPath)
    expectScreenshotsDiffer(hoverPath, activePath)
    expectScreenshotsDiffer(basePath, activePath)
  })
})
