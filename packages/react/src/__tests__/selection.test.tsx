/// Text selection through the real GPUI pipeline.
///
/// The window is 1024x768 (VisualTestAppContext default) and text lays out at
/// the window's default font size, so the coordinates here are deliberately
/// generous: a drag from the far left to the far right of a line selects the
/// whole line without depending on exact glyph advances.

import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot } from "../testing.js"

describe("text selection", () => {
  it("selects text inside one element", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <text style={{ fontSize: 20 }}>hello world</text>
      </div>
    )

    const selected = renderer.dragSelect(21, 30, 900, 30)
    expect(selected).toBe("hello world")
  })

  it("copies the selection with cmd-c", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <text style={{ fontSize: 20 }}>hello world</text>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBe("hello world")
    renderer.simulateKeystrokes("cmd-c")
    expect(renderer.readClipboardText()).toBe("hello world")
  })

  it("starts a selection in the empty space before the glyphs", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 40 }}>
        <text style={{ fontSize: 20 }}>hello world</text>
      </div>
    )

    // Parent padding sits outside every TextLayout bounds. The down must
    // still clamp to the nearest line, the same way a drag already does.
    expect(renderer.dragSelect(8, 50, 900, 50)).toBe("hello world")
    renderer.clearSelection()
    expect(renderer.dragSelect(8, 50, 8, 50)).toBeNull()
  })

  it("does not start a selection from a press far below the text", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 40 }}>
        <text style={{ fontSize: 20 }}>hello world</text>
      </div>
    )

    // Composer, titlebar, and empty chrome sit off the line. Starting there
    // must not claim the nearest paragraph.
    expect(renderer.dragSelect(50, 700, 900, 50)).toBeNull()
  })

  it("selects across sibling elements in document order", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, gap: 8 }}>
        <text style={{ fontSize: 20 }}>first line</text>
        <text style={{ fontSize: 20 }}>second line</text>
        <text style={{ fontSize: 20 }}>third line</text>
      </div>
    )

    // Dragging past the last line clamps to the nearest registered element,
    // which is how a drag into the gutter behaves in any text editor.
    const selected = renderer.dragSelect(21, 30, 900, 300)
    expect(selected).toBe("first line\nsecond line\nthird line")
  })

  it("takes a partial span from the anchor element and whole spans below", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, gap: 8 }}>
        <text style={{ fontSize: 20 }}>aaaaaaaaaa</text>
        <text style={{ fontSize: 20 }}>bbbb</text>
      </div>
    )

    const selected = renderer.dragSelect(21, 30, 900, 300)
    expect(selected).toBe("aaaaaaaaaa\nbbbb")

    renderer.clearSelection()
    // Start halfway through the first line: the anchor span must be partial.
    const partial = renderer.dragSelect(60, 30, 900, 300)
    expect(partial).not.toBeNull()
    expect(partial!.startsWith("aaaaaaaaaa")).toBe(false)
    expect(partial!.endsWith("\nbbbb")).toBe(true)
  })

  it("resolves a reversed drag the same way", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, gap: 8 }}>
        <text style={{ fontSize: 20 }}>alpha</text>
        <text style={{ fontSize: 20 }}>beta</text>
      </div>
    )

    const downward = renderer.dragSelect(21, 30, 900, 62)
    renderer.clearSelection()
    const upward = renderer.dragSelect(900, 62, 21, 30)
    expect(downward).toBe("alpha\nbeta")
    expect(upward).toBe(downward)
  })

  it("keeps text nested in styled divs selectable", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <div style={{ display: "flex", backgroundColor: "#1e1e2e", padding: 4 }}>
          <text style={{ fontSize: 20, color: "#cdd6f4" }}>nested text</text>
        </div>
      </div>
    )

    expect(renderer.dragSelect(25, 34, 900, 34)).toBe("nested text")
  })

  it("opts out of selection with userSelect none", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <text style={{ fontSize: 20, userSelect: "none" }}>untouchable</text>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBeNull()
  })

  it("inherits userSelect none from an ancestor", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, userSelect: "none" }}>
        <div style={{ display: "flex" }}>
          <text style={{ fontSize: 20 }}>toolbar label</text>
        </div>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBeNull()
  })

  it("re-enables selection under a userSelect none ancestor", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, userSelect: "none" }}>
        <text style={{ fontSize: 20, userSelect: "text" }}>selectable again</text>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBe("selectable again")
  })

  it("clears the selection", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <text style={{ fontSize: 20 }}>clear me</text>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBe("clear me")
    renderer.clearSelection()
    expect(renderer.getSelectedText()).toBeNull()
  })

  it("selects nothing for a click without movement", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <text style={{ fontSize: 20 }}>just a click</text>
      </div>
    )

    expect(renderer.dragSelect(40, 30, 40, 30)).toBeNull()
  })

  it("applies lineHeight to wrapped text", () => {
    const a = createTestRoot()
    a.render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, width: 200 }}>
        <text style={{ fontSize: 16, lineHeight: "18px" }}>
          one two three four five six seven eight nine ten
        </text>
      </div>
    )
    // A tight line height packs the wrapped lines closer, so a drag that ends
    // at a fixed y reaches further into the text.
    const tight = a.renderer.dragSelect(21, 26, 900, 60)

    const b = createTestRoot()
    b.render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, width: 200 }}>
        <text style={{ fontSize: 16, lineHeight: "40px" }}>
          one two three four five six seven eight nine ten
        </text>
      </div>
    )
    const loose = b.renderer.dragSelect(21, 26, 900, 60)

    expect(tight).not.toBeNull()
    expect(loose).not.toBeNull()
    expect(tight!.length).toBeGreaterThan(loose!.length)
  })

  it("selects text rendered as a div child, not only text nodes", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20, fontSize: 20 }}>
        <div>plain div text</div>
      </div>
    )

    expect(renderer.dragSelect(21, 30, 900, 30)).toBe("plain div text")
  })
})
