/// The native <code> element: syntax highlighting, line numbers, selection.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsDiffer, SHOTS_DIR } from "./test-utils.js"

// The GPU test renderer is macOS and Windows only.
const describeNative = hasNativeTestRenderer ? describe : describe.skip

const TS_SOURCE = `interface User {
  id: number
  name: string
}

export function greet(user: User): string {
  // Say hello.
  return \`hello \${user.name}\`
}`

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

/** Default `codeLineHeight`. One row per source line, at this exact height. */
const LINE_HEIGHT = 18

function codeBounds(renderer: { findByType(type: string): { id: number }[]; getElementBounds(id: number): { x: number; y: number; width: number; height: number } | null }) {
  const node = renderer.findByType("code")[0]
  expect(node).toBeDefined()
  const bounds = renderer.getElementBounds(node!.id)
  expect(bounds).not.toBeNull()
  return bounds!
}

describeNative("<code>", () => {
  it("renders one row per source line", () => {
    const { render, renderer } = createTestRoot()
    render(<code code={"a\nb\nc"} language="ts" />)

    // Only the code paints. No language header, no chrome of any kind.
    expect(renderer.getPaintedText()).toEqual(["a", "b", "c"])
  })

  it("normalizes CRLF source lines", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <code code={"// one\r\n// two\r\n"} language="ts" />
      </div>
    )

    expect(renderer.getPaintedText()).toEqual(["// one", "// two", ""])
    expect(renderer.dragSelect(22, 25, 900, 500)).toBe("// one\n// two")
  })

  it("paints no surface of its own", () => {
    const { render, renderer } = createTestRoot()
    render(<code code={"a\nb\nc"} language="ts" />)

    // Exactly the rows: no padding, no header strip, no border.
    expect(codeBounds(renderer).height).toBe(3 * LINE_HEIGHT)
  })

  it("keeps JSON-looking source strings as source text", () => {
    const cases = ["true", "null", '{"a":1}', "ordinary text"]
    for (const code of cases) {
      const { render, renderer } = createTestRoot()
      render(<code code={code} language="txt" />)
      expect(renderer.getPaintedText()).toContain(code)
    }
  })

  it("renders an empty code block without crashing", () => {
    const { render, renderer } = createTestRoot()
    render(<code code="" language="ts" />)
    expect(renderer.findByType("code")).toHaveLength(1)
  })

  it("never paints the language as a header", () => {
    const { render, renderer } = createTestRoot()
    render(<code code="x = 1" language="python" />)
    expect(renderer.getPaintedText()).not.toContain("python")
  })

  it("grows by the padding from the style prop", () => {
    const { render, renderer } = createTestRoot()
    render(<code code={"a\nb\nc"} language="ts" style={{ padding: 20 }} />)

    expect(codeBounds(renderer).height).toBe(3 * LINE_HEIGHT + 40)
  })

  it("takes the line height and font size from the style prop", () => {
    const { render, renderer } = createTestRoot()
    render(<code code={"a\nb\nc"} language="ts" style={{ fontSize: 20, lineHeight: 30 }} />)

    // The row height follows style.lineHeight, so tall glyphs are never clipped.
    expect(codeBounds(renderer).height).toBe(3 * 30)
  })

  it("scales the rows when only fontSize is given", () => {
    const { render, renderer } = createTestRoot()
    // Double the glyphs and the rows must double too, or the lines overlap.
    render(<code code={"a\nb\nc"} language="ts" style={{ fontSize: 25 }} />)

    expect(codeBounds(renderer).height).toBe(3 * 2 * LINE_HEIGHT)
  })

  it("paints the fill from the style prop", () => {
    const bare = path.join(SHOTS_DIR, "code-style-bare.png")
    const filled = path.join(SHOTS_DIR, "code-style-filled.png")

    const a = createTestRoot()
    a.render(
      <div style={{ display: "flex", padding: 24, backgroundColor: "#060606", height: "100%" }}>
        <code code={TS_SOURCE} language="typescript" />
      </div>
    )
    a.renderer.captureScreenshot(bare)

    const b = createTestRoot()
    b.render(
      <div style={{ display: "flex", padding: 24, backgroundColor: "#060606", height: "100%" }}>
        <code
          code={TS_SOURCE}
          language="typescript"
          style={{ padding: 12, borderRadius: 10, backgroundColor: "#1d1d1d" }}
        />
      </div>
    )
    b.renderer.captureScreenshot(filled)

    expectScreenshotsDiffer(bare, filled)
  })

  it("renders line numbers when asked", () => {
    const { render, renderer } = createTestRoot()
    render(<code code={"a\nb\nc"} language="ts" showLineNumbers />)

    // Gutter numbers paint before their line, so the log interleaves them.
    expect(renderer.getPaintedText()).toEqual(["1", "a", "2", "b", "3", "c"])
  })

  it("keeps code text selectable", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <code code={"const answer = 42"} language="ts" />
      </div>
    )

    const selected = renderer.dragSelect(22, 25, 900, 42)
    expect(selected).toBe("const answer = 42")
  })

  it("selects across several code lines", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <code code={"one\ntwo\nthree"} language="ts" />
      </div>
    )

    const selected = renderer.dragSelect(22, 25, 900, 500)
    expect(selected).toBe("one\ntwo\nthree")
  })

  it("does not select the line-number gutter", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <code code={"alpha\nbeta"} language="ts" showLineNumbers />
      </div>
    )

    // Anchor inside the code column, past the gutter, and drag to the end.
    const selected = renderer.dragSelect(70, 25, 900, 500)
    // The gutter painted this frame, but a drag must never pick it up: the
    // exact anchor column is font-dependent, the absence of digits is not.
    expect(renderer.getPaintedText()).toContain("1")
    expect(selected).not.toMatch(/\d/)
    expect(selected?.endsWith("beta")).toBe(true)
  })

  it("starts a selection in the gutter and still skips line numbers", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <code code={"alpha\nbeta"} language="ts" showLineNumbers />
      </div>
    )

    const selected = renderer.dragSelect(24, 25, 900, 500)
    expect(selected).toBe("alpha\nbeta")
  })

  it("changes appearance when a syntax theme is applied", () => {
    const before = path.join(SHOTS_DIR, "code-theme-default.png")
    const after = path.join(SHOTS_DIR, "code-theme-custom.png")

    const a = createTestRoot()
    a.render(
      <div style={{ display: "flex", padding: 24, backgroundColor: "#060606", height: "100%" }}>
        <code code={TS_SOURCE} language="typescript" showLineNumbers />
      </div>
    )
    a.renderer.captureScreenshot(before)

    const b = createTestRoot()
    b.render(
      <div style={{ display: "flex", padding: 24, backgroundColor: "#060606", height: "100%" }}>
        <code
          code={TS_SOURCE}
          language="typescript"
          showLineNumbers
          theme={{
            syntax: {
              keyword: "#ff0000",
              string: "#00ff00",
              typeName: "#0000ff",
              comment: "#ff00ff",
            },
          }}
        />
      </div>
    )
    b.renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })

  it("captures a reference screenshot of a highlighted block", () => {
    const shot = path.join(SHOTS_DIR, "code-typescript.png")
    const { render, renderer } = createTestRoot()
    render(
      <div
        style={{
          display: "flex",
          flexDirection: "column",
          padding: 32,
          backgroundColor: "#060606",
          height: "100%",
        }}
      >
        {/* The card is the caller's, built from `style` alone. */}
        <code
          code={TS_SOURCE}
          language="typescript"
          showLineNumbers
          style={{
            padding: 12,
            borderRadius: 10,
            borderWidth: 1,
            borderColor: "#ffffff1f",
            backgroundColor: "#ffffff09",
          }}
        />
      </div>
    )
    renderer.captureScreenshot(shot)

    expect(fs.existsSync(shot)).toBe(true)
    expect(fs.statSync(shot).size).toBeGreaterThan(0)
  })

  it("lets a parent scroller take a vertical wheel over a wide block", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ width: 240, height: 120, overflowY: "scroll" }}>
        <code
          code={"const wide = '".padEnd(200, "x") + "'"}
          language="ts"
        />
        <div style={{ height: 400 }}>
          <text>below</text>
        </div>
      </div>
    )

    const container = renderer
      .findByType("div")
      .find((d) => d.style.overflowY === "scroll")
    expect(container).toBeDefined()
    expect(renderer.getScrollOffset(container!.id)).toEqual([0, 0])

    renderer.nativeSimulateScrollWheel(80, 50, 0, -80)
    const offset = renderer.getScrollOffset(container!.id)
    expect(offset).not.toBeNull()
    expect(offset![1]).toBeLessThan(0)
  })
})
