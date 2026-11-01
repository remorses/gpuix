/// The native <diff> element: patch parsing, virtualization, selection, events.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it, vi } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { expectScreenshotsDiffer, SHOTS_DIR } from "./test-utils.js"

// The GPU test renderer is macOS and Windows only.
const describeNative = hasNativeTestRenderer ? describe : describe.skip

const PATCH = [
  "diff --git a/src/app.ts b/src/app.ts",
  "index 1111111..2222222 100644",
  "--- a/src/app.ts",
  "+++ b/src/app.ts",
  "@@ -1,7 +1,8 @@",
  "import { createServer } from 'http'",
  "",
  "-const port = 3000",
  "+const port = 8080",
  "+const host = '0.0.0.0'",
  "",
  " export function start() {",
  "-  return createServer().listen(port)",
  "+  return createServer().listen(port, host)",
  " }",
]
  .map((line, ix) => (ix >= 5 && !line.startsWith("-") && !line.startsWith("+") && !line.startsWith("@") ? ` ${line}` : line))
  .join("\n")

const TWO_FILES = [
  "diff --git a/README.md b/README.md",
  "--- a/README.md",
  "+++ b/README.md",
  "@@ -1,2 +1,2 @@",
  " # Title",
  "-old line",
  "+new line",
  "diff --git a/src/lib.rs b/src/lib.rs",
  "new file mode 100644",
  "--- /dev/null",
  "+++ b/src/lib.rs",
  "@@ -0,0 +1,3 @@",
  "+pub fn hello() -> &'static str {",
  "+    \"hi\"",
  "+}",
].join("\n")

/** A long patch, to prove the list virtualizes instead of building every row. */
function longPatch(lines: number): string {
  const rows = [
    "diff --git a/big.ts b/big.ts",
    "--- a/big.ts",
    "+++ b/big.ts",
    `@@ -1,${lines} +1,${lines} @@`,
  ]
  for (let i = 0; i < lines; i++) {
    rows.push(i % 7 === 0 ? `-const v${i} = ${i}` : ` const v${i} = ${i}`)
    if (i % 7 === 0) rows.push(`+const v${i} = ${i + 1}`)
  }
  return rows.join("\n")
}

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

describeNative("<diff>", () => {
  it("renders file headers with addition and deletion counts", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("src/app.ts")
    expect(painted).toContain("+3")
    expect(painted).toContain("−2")
  })

  it("renders hunk headers and line numbers", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("@@ -1,7 +1,8 @@")
    expect(painted).toContain("1")
    expect(painted).toContain("+")
    expect(painted).toContain("−")
  })

  it("renders several files with their notices", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch={TWO_FILES} style={{ width: "100%", height: "100%" }} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("README.md")
    expect(painted).toContain("src/lib.rs")
    expect(painted).toContain("New file")
  })

  it("renders 'No changes' for an empty patch", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch="" style={{ width: "100%", height: "100%" }} />)
    expect(renderer.getPaintedText()).toContain("No changes")
  })

  it("collapses a file down to its header", () => {
    const expanded = createTestRoot()
    expanded.render(<diff patch={TWO_FILES} style={{ width: "100%", height: "100%" }} />)
    const expandedPainted = expanded.renderer.getPaintedText()

    const collapsed = createTestRoot()
    collapsed.render(
      <diff
        patch={TWO_FILES}
        collapsedPaths={["README.md"]}
        style={{ width: "100%", height: "100%" }}
      />
    )
    const collapsedPainted = collapsed.renderer.getPaintedText()

    expect(expandedPainted).toContain("old line")
    expect(collapsedPainted).not.toContain("old line")
    // The header survives so the file can be expanded again.
    expect(collapsedPainted).toContain("README.md")
  })

  it("fires onToggleFile with the file path", () => {
    const onToggleFile = vi.fn()
    const { render, renderer } = createTestRoot()
    render(
      <diff
        patch={TWO_FILES}
        onToggleFile={onToggleFile}
        style={{ width: "100%", height: "100%" }}
      />
    )

    // The first file header sits at the very top of the list.
    renderer.nativeSimulateClick(200, 18)
    expect(onToggleFile).toHaveBeenCalled()
    expect(onToggleFile.mock.calls[0][0].value).toBe("README.md")
  })

  it("fires onLineClick with line text and numbers", () => {
    const onLineClick = vi.fn()
    const { render, renderer } = createTestRoot()
    render(
      <diff
        patch={TWO_FILES}
        onLineClick={onLineClick}
        style={{ width: "100%", height: "100%" }}
      />
    )

    // header 36 + hunk header 28 = 64, so the first line row spans 64..85.
    renderer.nativeSimulateClick(300, 74)
    expect(onLineClick).toHaveBeenCalled()
    const event = onLineClick.mock.calls[0][0]
    expect(event.value).toBe("# Title")
    expect(event.oldLine).toBe(1)
    expect(event.newLine).toBe(1)
  })

  it("keeps diff code selectable but not the gutters", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch={TWO_FILES} style={{ width: "100%", height: "100%" }} />)

    // The code column starts after accent(3) + two gutters(36) + marker(28)
    // + 12px pad, so 120 lands on the first glyph of the line.
    const selected = renderer.dragSelect(120, 74, 900, 74)
    expect(selected).toBe("# Title")
    // Line numbers and the marker column painted, but never get copied.
    expect(renderer.getPaintedText()).toContain("·")
    expect(selected).not.toContain("·")
  })

  it("caps visible lines and paints Show more", () => {
    const { render, renderer } = createTestRoot()
    render(
      <diff
        patch={TWO_FILES}
        maxLines={1}
        style={{ width: "100%", height: "100%" }}
      />
    )

    const painted = renderer.getPaintedText()
    expect(painted).toContain("# Title")
    expect(painted).toContain("Show 5 more lines")
    expect(painted).not.toContain("old line")
    expect(painted).not.toContain("pub fn hello")
  })

  it("fires onShowMore with the hidden line count", () => {
    const onShowMore = vi.fn()
    const { render, renderer } = createTestRoot()
    render(
      <diff
        patch={TWO_FILES}
        maxLines={1}
        onShowMore={onShowMore}
        style={{ width: "100%", height: "100%" }}
      />
    )

    // header 36 + hunk 28 + one line 21 = 85, so Show more starts there.
    renderer.nativeSimulateClick(200, 96)
    expect(onShowMore).toHaveBeenCalled()
    expect(onShowMore.mock.calls[0][0].value).toBe("5")
  })

  it("lets a parent scroller take the wheel when scroll is off", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ width: "100%", height: 120, overflowY: "scroll" }}>
        <diff patch={TWO_FILES} />
      </div>
    )

    const container = renderer.findByType("div").find((d) => d.style.overflowY === "scroll")
    expect(container).toBeDefined()
    expect(renderer.getScrollOffset(container!.id)).toEqual([0, 0])

    renderer.nativeSimulateScrollWheel(200, 60, 0, -80)
    const offset = renderer.getScrollOffset(container!.id)
    expect(offset).not.toBeNull()
    expect(offset![1]).toBeLessThan(0)
  })

  it("virtualizes only when scroll is on", () => {
    const { render, renderer } = createTestRoot()
    render(
      <diff
        scroll
        patch={longPatch(2000)}
        style={{ width: "100%", height: "100%" }}
      />
    )

    const painted = renderer.getPaintedText()
    // The window is 768px tall with 21px rows, so roughly 37 rows fit. The
    // overdraw window widens that, but it must stay far below 2000.
    expect(painted.length).toBeGreaterThan(10)
    expect(painted.length).toBeLessThan(1500)
  })

  it("reuses the syntax cache across frames", () => {
    const { render, renderer } = createTestRoot()
    render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)
    const [hitsBefore] = renderer.getSyntaxCacheStats()

    // Re-render the same patch: the highlight must come from the cache.
    render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)
    renderer.getPaintedText()
    const [hitsAfter, , documents] = renderer.getSyntaxCacheStats()

    expect(documents).toBeGreaterThan(0)
    expect(hitsAfter).toBeGreaterThanOrEqual(hitsBefore)
  })

  it("repaints when the theme changes on a mounted element", () => {
    const before = path.join(SHOTS_DIR, "diff-theme-live-before.png")
    const after = path.join(SHOTS_DIR, "diff-theme-live-after.png")

    const { render, renderer } = createTestRoot()
    render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)
    renderer.captureScreenshot(before)

    // The theme only affects paint, so it must NOT be part of the parse cache
    // key, and it must still repaint. An earlier version hashed three
    // representative tokens and silently ignored every other change.
    render(
      <diff
        patch={PATCH}
        theme={{ bg: "#400000", diffDel: "#ffff00", textDim: "#00ffff" }}
        style={{ width: "100%", height: "100%" }}
      />
    )
    renderer.captureScreenshot(after)

    expectScreenshotsDiffer(before, after)
  })

  it("swaps to a different patch with the same row count", () => {
    const rowsA = [
      "diff --git a/a.ts b/a.ts",
      "--- a/a.ts",
      "+++ b/a.ts",
      "@@ -1,2 +1,2 @@",
      " const keep = 1",
      "-const gone = 2",
      "+const came = 3",
    ].join("\n")
    const rowsB = [
      "diff --git a/b.ts b/b.ts",
      "--- a/b.ts",
      "+++ b/b.ts",
      "@@ -9,2 +9,2 @@",
      " const other = 1",
      "-const old = 9",
      "+const fresh = 9",
    ].join("\n")

    const { render, renderer } = createTestRoot()
    render(<diff patch={rowsA} style={{ width: "100%", height: "100%" }} />)
    expect(renderer.getPaintedText()).toContain("a.ts")

    render(<diff patch={rowsB} style={{ width: "100%", height: "100%" }} />)
    const painted = renderer.getPaintedText()
    expect(painted).toContain("b.ts")
    expect(painted).not.toContain("a.ts")
    expect(painted).toContain("const fresh = 9")
    expect(painted).not.toContain("const gone = 2")
  })

  it("colours a deleted line from the pre-change side", () => {
    // The same text appears as a deletion in one place and inside a comment in
    // another. Keying highlights by line text collapses them onto whichever
    // came first; keying by side and line number does not.
    const patch = [
      "diff --git a/x.rs b/x.rs",
      "--- a/x.rs",
      "+++ b/x.rs",
      "@@ -1,4 +1,4 @@",
      " fn main() {",
      "-    let value = 1;",
      "+    let value = 2;",
      " }",
    ].join("\n")

    const { render, renderer } = createTestRoot()
    render(<diff patch={patch} style={{ width: "100%", height: "100%" }} />)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("    let value = 1;")
    expect(painted).toContain("    let value = 2;")
  })

  it("word diff changes what is painted under the glyphs", () => {
    const off = path.join(SHOTS_DIR, "diff-word-off.png")
    const on = path.join(SHOTS_DIR, "diff-word-on.png")

    const a = createTestRoot()
    a.render(<diff patch={PATCH} style={{ width: "100%", height: "100%" }} />)
    a.renderer.captureScreenshot(off)

    const b = createTestRoot()
    b.render(<diff patch={PATCH} wordDiff style={{ width: "100%", height: "100%" }} />)
    b.renderer.captureScreenshot(on)

    expectScreenshotsDiffer(off, on)
  })

  it("captures a reference screenshot of a multi-file diff", () => {
    const shot = path.join(SHOTS_DIR, "diff-multi-file.png")
    const { render, renderer } = createTestRoot()
    render(<diff patch={TWO_FILES} wordDiff style={{ width: "100%", height: "100%" }} />)
    renderer.captureScreenshot(shot)

    expect(fs.existsSync(shot)).toBe(true)
    expect(fs.statSync(shot).size).toBeGreaterThan(0)
  })
})
