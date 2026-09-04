/// Custom properties in the `style` prop, and `var()` reading them.
///
/// Each case paints twice. Once through a variable, once with the value
/// written in place. The two screenshots have to be byte-identical, because a
/// variable is a name for text that the property parser then reads as if the
/// author had written it there.
///
/// The counter tests at the end matter as much as the pixels. A variable makes
/// an element's resolved style depend on its ancestors, so the cache has to
/// stay correct without giving up on elements that use no variable at all.

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

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `var-${name}.png`)

const BOX = { width: 200, height: 120 } as const

/// Paint `tree`, save it under `name`, and tear the root down.
function paint(name: string, tree: React.ReactElement) {
  const root = createTestRoot()
  root.render(tree)
  root.renderer.captureScreenshot(shot(name))
  root.unmount()
}

describeNative("custom properties", () => {
  it("paints a colour read through a variable", () => {
    paint(
      "own-through",
      <div style={{ ...BOX, "--brand": "#ff0000", backgroundColor: "var(--brand)" }} />
    )
    paint("own-direct", <div style={{ ...BOX, backgroundColor: "#ff0000" }} />)
    expectScreenshotsEqual(shot("own-through"), shot("own-direct"))
  })

  it("reads a variable an ancestor declared", () => {
    paint(
      "ancestor-through",
      <div style={{ "--brand": "#0000ff" }}>
        <div style={{ ...BOX, backgroundColor: "var(--brand)" }} />
      </div>
    )
    paint("ancestor-direct", <div style={{ ...BOX, backgroundColor: "#0000ff" }} />)
    expectScreenshotsEqual(shot("ancestor-through"), shot("ancestor-direct"))
  })

  it("takes the nearest declaration when two ancestors disagree", () => {
    paint(
      "nearest-through",
      <div style={{ "--brand": "#ff0000" }}>
        <div style={{ "--brand": "#00ff00" }}>
          <div style={{ ...BOX, backgroundColor: "var(--brand)" }} />
        </div>
      </div>
    )
    paint("nearest-direct", <div style={{ ...BOX, backgroundColor: "#00ff00" }} />)
    expectScreenshotsEqual(shot("nearest-through"), shot("nearest-direct"))
  })

  it("uses the fallback when nothing declared the variable", () => {
    paint(
      "fallback-through",
      <div style={{ ...BOX, backgroundColor: "var(--nope, #00ff00)" }} />
    )
    paint("fallback-direct", <div style={{ ...BOX, backgroundColor: "#00ff00" }} />)
    expectScreenshotsEqual(shot("fallback-through"), shot("fallback-direct"))
  })

  it("leaves the property unset when the variable is missing", () => {
    // CSS calls this invalid at computed-value time. The element keeps the
    // value it would have had, which here is no background of its own.
    paint("missing-through", <div style={{ ...BOX, backgroundColor: "var(--nope)" }} />)
    paint("missing-direct", <div style={BOX} />)
    expectScreenshotsEqual(shot("missing-through"), shot("missing-direct"))
  })

  it("repaints a subtree when a declaration above it changes", () => {
    const root = createTestRoot()
    const tree = (brand: string) => (
      <div style={{ "--brand": brand }}>
        <div style={{ ...BOX, backgroundColor: "var(--brand)" }} />
      </div>
    )

    root.render(tree("#ff0000"))
    root.renderer.captureScreenshot(shot("change-before"))
    root.render(tree("#00ff00"))
    root.renderer.captureScreenshot(shot("change-after"))
    root.unmount()

    paint("change-expected", <div style={{ ...BOX, backgroundColor: "#00ff00" }} />)
    expectScreenshotsEqual(shot("change-after"), shot("change-expected"))
  })

  it("paints a border with the colour currentColor names", () => {
    paint(
      "current-through",
      <div
        style={{
          ...BOX,
          color: "#ff0000",
          borderWidth: 6,
          borderColor: "currentColor",
        }}
      />
    )
    paint(
      "current-direct",
      <div style={{ ...BOX, color: "#ff0000", borderWidth: 6, borderColor: "#ff0000" }} />
    )
    expectScreenshotsEqual(shot("current-through"), shot("current-direct"))
  })

  it("takes currentColor from an ancestor when the element declares none", () => {
    paint(
      "current-inherited",
      <div style={{ color: "#0000ff" }}>
        <div style={{ ...BOX, borderWidth: 6, borderColor: "currentColor" }} />
      </div>
    )
    paint(
      "current-inherited-direct",
      <div style={{ ...BOX, borderWidth: 6, borderColor: "#0000ff" }} />
    )
    expectScreenshotsEqual(shot("current-inherited"), shot("current-inherited-direct"))
  })

  it("paints a length read through a variable", () => {
    paint(
      "length-through",
      <div style={{ "--pad": "20px", padding: "var(--pad)", backgroundColor: "#ff0000" }}>
        <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
      </div>
    )
    paint(
      "length-direct",
      <div style={{ padding: 20, backgroundColor: "#ff0000" }}>
        <div style={{ width: 40, height: 40, backgroundColor: "#00ff00" }} />
      </div>
    )
    expectScreenshotsEqual(shot("length-through"), shot("length-direct"))
  })

  it("takes a length written with its unit", () => {
    paint("unit-px", <div style={{ ...BOX, borderWidth: "6px", borderColor: "#ff0000" }} />)
    paint("unit-bare", <div style={{ ...BOX, borderWidth: 6, borderColor: "#ff0000" }} />)
    paint("unit-none", <div style={{ ...BOX, borderColor: "#ff0000" }} />)
    // The border has to be visible, or the comparison below is empty.
    expectScreenshotsDiffer(shot("unit-bare"), shot("unit-none"))
    expectScreenshotsEqual(shot("unit-px"), shot("unit-bare"))
  })

  it("reads rem against the root font size", () => {
    paint("unit-rem", <div style={{ ...BOX, borderWidth: "2rem", borderColor: "#ff0000" }} />)
    paint("unit-rem-direct", <div style={{ ...BOX, borderWidth: 32, borderColor: "#ff0000" }} />)
    expectScreenshotsEqual(shot("unit-rem"), shot("unit-rem-direct"))
  })

  it("drops a length it cannot read", () => {
    // Painting something arbitrary would be worse than painting nothing, so a
    // value the parser rejects leaves the property alone.
    paint("unit-bad", <div style={{ ...BOX, borderWidth: "banana", borderColor: "#ff0000" }} />)
    paint("unit-bare-again", <div style={{ ...BOX, borderColor: "#ff0000" }} />)
    expectScreenshotsEqual(shot("unit-bad"), shot("unit-bare-again"))
  })

  it("resolves a variable inside hover", () => {
    // The state resolves against the element's own scope, so a declaration on
    // the element is in scope for the `var()` in its hover style.
    const { renderer, render } = createTestRoot()
    render(
      <div
        style={{
          ...BOX,
          "--brand": "#ff0000",
          backgroundColor: "#111111",
          hover: { backgroundColor: "var(--brand)" },
        }}
      />
    )
    expect(renderer.styleResolutions()).toBeGreaterThan(0)
  })
})

describeNative("custom properties and the resolve cache", () => {
  it("resolves nothing on a repeat frame under a declaration", () => {
    // A variable makes an element depend on its ancestors. If the cascade were
    // rebuilt every frame the whole subtree below a declaration would resolve
    // again every frame, which is what this catches.
    const { renderer, render } = createTestRoot()
    render(
      <div style={{ "--brand": "#ff0000" }}>
        {Array.from({ length: 20 }, (_, i) => (
          <div key={i} style={{ width: 10, height: 10, backgroundColor: "var(--brand)" }} />
        ))}
      </div>
    )

    renderer.resetStyleResolutions()
    for (let i = 0; i < 10; i++) {
      renderer.flush()
    }

    expect(renderer.styleResolutions()).toBe(0)
  })

  it("re-resolves only the readers when a declaration changes", () => {
    const { renderer, render } = createTestRoot()
    const tree = (brand: string) => (
      <div style={{ "--brand": brand }}>
        <div style={{ width: 10, height: 10, backgroundColor: "var(--brand)" }} />
        <div style={{ width: 10, height: 10, backgroundColor: "var(--brand)" }} />
        <div style={{ width: 10, height: 10, backgroundColor: "#222222" }} />
      </div>
    )

    render(tree("#ff0000"))
    renderer.resetStyleResolutions()
    render(tree("#00ff00"))

    // The declaring div re-resolves because its own style changed, and the two
    // readers re-resolve because their scope did. The third child reads no
    // variable, so its cached resolution still holds.
    expect(renderer.styleResolutions()).toBe(3)
  })

  it("leaves a sibling subtree alone when a declaration changes", () => {
    const { renderer, render } = createTestRoot()
    const tree = (brand: string) => (
      <div>
        <div style={{ "--brand": brand }}>
          <div style={{ width: 10, height: 10, backgroundColor: "var(--brand)" }} />
        </div>
        <div style={{ "--other": "#0000ff" }}>
          {Array.from({ length: 10 }, (_, i) => (
            <div key={i} style={{ width: 10, height: 10, backgroundColor: "var(--other)" }} />
          ))}
        </div>
      </div>
    )

    render(tree("#ff0000"))
    renderer.resetStyleResolutions()
    render(tree("#00ff00"))

    // The declaring div and its one reader. The ten elements under the other
    // declaration never see a changed scope.
    expect(renderer.styleResolutions()).toBe(2)
  })
})
