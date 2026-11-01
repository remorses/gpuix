/// The native <markdown> element: blocks, inline styling, selection, links.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it, vi } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsDiffer, SHOTS_DIR } from "./test-utils.js"

// The GPU test renderer is macOS and Windows only.
const describeNative = hasNativeTestRenderer ? describe : describe.skip

const DOC = [
  "# GPUIX",
  "",
  "Build **native** desktop apps with *React*, rendered on the `GPU`.",
  "",
  "## Features",
  "",
  "- Selectable text everywhere",
  "- Tree-sitter syntax highlighting",
  "- Virtualized diffs",
  "",
  "> Immediate mode aligns with React's model.",
  "",
  "```ts",
  "const root = createRoot(renderer)",
  "root.render(<App />)",
  "```",
  "",
  "| Element | Purpose |",
  "|:--------|--------:|",
  "| code | syntax highlighting |",
  "| diff | unified patches |",
  "",
  "---",
  "",
  "See https://github.com/remorses/gpuix for more.",
].join("\n")

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

describeNative("<markdown>", () => {
  it("renders headings and paragraphs in document order", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source={"# Title\n\nSome body text."} />)

    expect(renderer.getPaintedText()).toEqual(["Title", "Some body text."])
  })

  it("flattens inline styling into one painted string per paragraph", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source="Build **native** apps with `code`." />)

    // Bold and code change fonts and colours, never the flattened text.
    expect(renderer.getPaintedText()).toEqual(["Build native apps with code."])
  })

  it("renders list items with their markers", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source={"- one\n- two\n\n1. first\n2. second"} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("one")
    expect(painted).toContain("two")
    // Ordered markers paint as chrome text; unordered ones are drawn discs.
    expect(painted).toContain("1.")
    expect(painted).toContain("2.")
  })

  it("renders fenced code blocks one line at a time", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source={"```ts\nconst a = 1\nconst b = 2\n```"} />)

    // Language tag, then one entry per line, with no phantom trailing line.
    expect(renderer.getPaintedText()).toEqual(["ts", "const a = 1", "const b = 2"])
  })

  it("renders tables cell by cell", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source={"| a | b |\n|---|---|\n| 1 | 2 |"} />)

    expect(renderer.getPaintedText()).toEqual(["a", "b", "1", "2"])
  })

  it("renders block quotes with their nested blocks", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source={"> quoted line\n>\n> - inner item"} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("quoted line")
    expect(painted).toContain("inner item")
  })

  it("renders nothing for an empty source", () => {
    const { render, renderer } = createTestRoot()
    render(<markdown source="" />)
    expect(renderer.getPaintedText()).toEqual([])
  })

  it("keeps markdown text selectable", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <markdown source="hello selectable world" />
      </div>
    )

    expect(renderer.dragSelect(22, 31, 900, 31)).toBe("hello selectable world")
  })

  it("selects across blocks, including into a code fence", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <markdown source={"first paragraph\n\n```\ncode line\n```"} />
      </div>
    )

    const selected = renderer.dragSelect(22, 31, 900, 600)
    expect(selected).toBe("first paragraph\ncode line")
  })

  it("does not select ordered-list markers", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <markdown source={"1. alpha\n2. beta"} />
      </div>
    )

    const selected = renderer.dragSelect(48, 31, 900, 600)
    expect(renderer.getPaintedText()).toContain("1.")
    expect(selected).not.toMatch(/\d/)
    expect(selected?.endsWith("beta")).toBe(true)
  })

  it("autolinks bare URLs and reports them on click", () => {
    const onLinkClick = vi.fn()
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <markdown source="see https://example.com/docs now" onLinkClick={onLinkClick} />
      </div>
    )

    renderer.nativeSimulateClick(100, 31)
    expect(onLinkClick).toHaveBeenCalled()
    expect(onLinkClick.mock.calls[0][0].value).toBe("https://example.com/docs")
  })

  it("hit-tests links per byte range, not per block", () => {
    const onLinkClick = vi.fn()
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ display: "flex", flexDirection: "column", padding: 20 }}>
        <markdown
          source="[one](https://one.test) plain [two](https://two.test)"
          onLinkClick={onLinkClick}
        />
      </div>
    )

    // Far past the end of the paragraph: prose, not a link.
    renderer.nativeSimulateClick(700, 30)
    expect(onLinkClick).not.toHaveBeenCalled()

    // The first link starts at the left edge of the text.
    renderer.nativeSimulateClick(24, 30)
    expect(onLinkClick).toHaveBeenCalledTimes(1)
    expect(onLinkClick.mock.calls[0][0].value).toBe("https://one.test")
  })

  it("re-renders when the theme prop changes on a mounted element", () => {
    const before = path.join(SHOTS_DIR, "markdown-theme-live-before.png")
    const after = path.join(SHOTS_DIR, "markdown-theme-live-after.png")

    const { render, renderer } = createTestRoot()
    render(<markdown source={DOC} />)
    renderer.captureScreenshot(before)

    // Same renderer, same element: a theme swap must repaint.
    render(<markdown source={DOC} theme={{ accent: "#ff0000", text: "#00ff00" }} />)
    renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })

  it("changes appearance when the theme is overridden", () => {
    const before = path.join(SHOTS_DIR, "markdown-theme-default.png")
    const after = path.join(SHOTS_DIR, "markdown-theme-custom.png")

    const a = createTestRoot()
    a.render(
      <div style={{ display: "flex", padding: 28, backgroundColor: "#060606", height: "100%" }}>
        <markdown source={DOC} />
      </div>
    )
    a.renderer.captureScreenshot(before)

    const b = createTestRoot()
    b.render(
      <div style={{ display: "flex", padding: 28, backgroundColor: "#060606", height: "100%" }}>
        <markdown source={DOC} theme={{ accent: "#ff8800", codeText: "#00ffcc" }} />
      </div>
    )
    b.renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })

  it("captures a reference screenshot of a full document", () => {
    const shot = path.join(SHOTS_DIR, "markdown-document.png")
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
        <markdown source={DOC} />
      </div>
    )
    renderer.captureScreenshot(shot)

    expect(fs.existsSync(shot)).toBe(true)
    expect(fs.statSync(shot).size).toBeGreaterThan(0)
  })
})
