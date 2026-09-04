import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

function binaryFrame(text: string, color = "#e7e7e7") {
  const bytes = new Uint8Array(100 * 2 * 16)
  const view = new DataView(bytes.buffer)
  const foreground = Number.parseInt(color.slice(1), 16)
  const background = 0x0b0c0c
  const glyphs = [...text.padEnd(200)]
  for (let index = 0; index < 200; index += 1) {
    const offset = index * 16
    view.setUint32(offset, glyphs[index]!.codePointAt(0)!, true)
    view.setUint32(offset + 4, foreground, true)
    view.setUint32(offset + 8, background, true)
  }
  return {
    version: 2 as const,
    cols: 100,
    rows: 2,
    cellWidth: 7.83,
    lineHeight: 17,
    fontSize: 13,
    background: "#0b0c0c",
    cursorColor: "#e7e7e7",
    cursorX: 0,
    cursorY: 1,
    cursorVisible: true,
    fontFamily: "Menlo",
    nerdFontFamily: "Symbols Nerd Font Mono",
    ligaturesEnabled: true,
    cells: bytes,
    graphemes: [],
  }
}

function frame(text: string, color = "#e7e7e7") {
  const binary = binaryFrame(text, color)
  return { ...binary, cells: Buffer.from(binary.cells).toString("base64") }
}

describeNative("custom element: terminal", () => {
  let root: TestRoot

  beforeEach(() => {
    root = createTestRoot({ width: 800, height: 50 })
  })

  afterEach(() => {
    root.unmount()
  })

  it("paints a compact fixed-cell frame through one native host", () => {
    const line = `┌${"─".repeat(98)}┐`
    root.render(
      <terminal
        testId="terminal"
        frame={frame(line)}
        style={{ width: 783, height: 34, overflow: "hidden" }}
      />
    )
    root.renderer.flush()

    expect(root.renderer.supportsNativeTerminal()).toBe(true)
    expect(root.renderer.findByType("terminal")).toHaveLength(1)
    expect(root.renderer.getPaintedText()).toContain(line)
    expect(root.renderer.getRetainedElementCount()).toBe(1)
  })

  it("stages only the latest direct binary frame before paint", () => {
    root.render(
      <terminal testId="direct-terminal" style={{ width: 783, height: 34 }} />
    )
    const terminal = root.renderer.findByTestId("direct-terminal")!
    const first = binaryFrame("B".repeat(100), "#ff6369")
    const latest = binaryFrame("C".repeat(100), "#70d083")
    const { cells: firstCells, ...firstMetadata } = first
    const { cells: latestCells, ...latestMetadata } = latest

    root.renderer.setTerminalFrame(terminal.id, JSON.stringify(firstMetadata), firstCells)
    root.renderer.setTerminalFrame(terminal.id, JSON.stringify(latestMetadata), latestCells)
    root.renderer.flush()

    expect(root.renderer.getPaintedText()).toContain("C".repeat(100))
    expect(root.renderer.getPaintedText()).not.toContain("B".repeat(100))
    expect(root.renderer.getRetainedElementCount()).toBe(1)
  })

  it("updates text and style without creating per-cell retained nodes", () => {
    root.render(
      <terminal frame={frame("A".repeat(100), "#ff6369")} style={{ width: 783, height: 34 }} />
    )
    root.renderer.flush()
    root.render(
      <terminal frame={frame("B".repeat(100), "#70d083")} style={{ width: 783, height: 34 }} />
    )
    root.renderer.flush()

    expect(root.renderer.getPaintedText()).toContain("B".repeat(100))
    expect(root.renderer.getRetainedElementCount()).toBe(1)
  })
})
