/**
 * Visual tests for the GPUIX diff viewer example.
 *
 * Uses the native GPUI test renderer (real Metal rendering on macOS) to
 * validate diff rendering end-to-end: React → Rust RetainedTree → GpuixView →
 * GPUI layout → Metal → screenshot.
 *
 * Each test renders a diff, takes a screenshot, and verifies text content.
 * Screenshots are saved to /tmp/gpuix-diff-*.png for manual inspection.
 *
 * @ts-nocheck
 */

import fs from "fs"
import { describe, it, expect, beforeEach } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer } from "@gpuix/react"
import type { StructuredPatchHunk as Hunk } from "diff"
import { DiffViewer } from "./diff"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const SCREENSHOT_DIR = "/tmp"

// Background for the scroll container — matches the diff viewer's unchanged line bg
// so the area below content doesn't look different.
const DIFF_BG = "rgba(15, 15, 15, 1)"

// ── Test fixtures ────────────────────────────────────────────────────

const simpleHunks: Hunk[] = [
  {
    oldStart: 1,
    oldLines: 4,
    newStart: 1,
    newLines: 4,
    lines: [
      " const x = 1;",
      "-const y = 2;",
      "+const y = 3;",
      " const z = x + y;",
    ],
  },
]

const multiHunkPatch: Hunk[] = [
  {
    oldStart: 1,
    oldLines: 5,
    newStart: 1,
    newLines: 7,
    lines: [
      " import React from 'react'",
      " import { useState } from 'react'",
      " ",
      "-function Counter({ initial }: { initial: number }) {",
      "-  const [count, setCount] = useState(initial)",
      "+interface CounterProps {",
      "+  initial: number",
      "+  step?: number",
      "+}",
      "+",
      "+function Counter({ initial, step = 1 }: CounterProps) {",
      "+  const [count, setCount] = useState(initial)",
    ],
  },
  {
    oldStart: 10,
    oldLines: 4,
    newStart: 12,
    newLines: 6,
    lines: [
      "       <span>{count}</span>",
      "-      <button onClick={() => setCount(c => c + 1)}>+</button>",
      "-      <button onClick={() => setCount(c => c - 1)}>-</button>",
      "+      <button onClick={() => setCount(c => c + step)}>",
      "+        Increment by {step}",
      "+      </button>",
      "+      <button onClick={() => setCount(c => c - step)}>",
      "+        Decrement by {step}",
      "+      </button>",
      "     </div>",
    ],
  },
]

const longHunk: Hunk[] = [
  {
    oldStart: 1,
    oldLines: 30,
    newStart: 1,
    newLines: 32,
    lines: [
      " // Line 1: imports",
      " import fs from 'fs'",
      " import path from 'path'",
      " ",
      "-const VERSION = '1.0.0'",
      "+const VERSION = '2.0.0'",
      " ",
      " function readFile(name: string) {",
      "   const full = path.join(__dirname, name)",
      "   return fs.readFileSync(full, 'utf-8')",
      " }",
      " ",
      " function writeFile(name: string, data: string) {",
      "-  fs.writeFileSync(name, data)",
      "+  const full = path.join(__dirname, name)",
      "+  fs.writeFileSync(full, data, 'utf-8')",
      " }",
      " ",
      " function processAll() {",
      "   const files = fs.readdirSync('.')",
      "   for (const file of files) {",
      "     const content = readFile(file)",
      "-    console.log(file, content.length)",
      "+    const processed = content.trim()",
      "+    console.log(file, processed.length)",
      "   }",
      " }",
      " ",
      " export { readFile, writeFile, processAll }",
      " export default { VERSION }",
    ],
  },
]

// ── Tests ────────────────────────────────────────────────────────────

describeNative("diff viewer", () => {
  let testRoot: ReturnType<typeof createTestRoot>

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  describe("unified view", () => {
    it("renders a simple unified diff with syntax highlighting", () => {
      function UnifiedSimple() {
        return (
          <div
            style={{
              width: "100%",
              height: "100%",
              backgroundColor: DIFF_BG,
              overflow: "scroll",
            }}
          >
            <DiffViewer
              hunks={simpleHunks}
              filePath="test.ts"
              splitView={false}
            />
          </div>
        )
      }

      testRoot.render(<UnifiedSimple />)

      // Verify key text content is present
      const allText = testRoot.renderer.getAllText()
      // Line numbers and code should be present
      expect(allText.some((t: string) => t.includes("const"))).toBe(true)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-unified-simple.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("renders multi-hunk diff with hunk separators", () => {
      function UnifiedMultiHunk() {
        return (
          <div
            style={{
              width: "100%",
              height: "100%",
              backgroundColor: DIFF_BG,
              overflow: "scroll",
            }}
          >
            <DiffViewer
              hunks={multiHunkPatch}
              filePath="counter.tsx"
              splitView={false}
            />
          </div>
        )
      }

      testRoot.render(<UnifiedMultiHunk />)

      const allText = testRoot.renderer.getAllText()
      // Should contain both hunks' content
      expect(allText.some((t: string) => t.includes("import"))).toBe(true)
      expect(allText.some((t: string) => t.includes("button"))).toBe(true)
      // Should have hunk separator
      expect(allText.some((t: string) => t.includes("..."))).toBe(true)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-unified-multi.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("split view", () => {
    it("renders split diff with left/right panes", () => {
      function SplitSimple() {
        return (
          <div
            style={{
              width: "100%",
              height: "100%",
              backgroundColor: DIFF_BG,
              overflow: "scroll",
            }}
          >
            <DiffViewer
              hunks={simpleHunks}
              filePath="test.ts"
              splitView={true}
            />
          </div>
        )
      }

      testRoot.render(<SplitSimple />)

      const allText = testRoot.renderer.getAllText()
      expect(allText.some((t: string) => t.includes("const"))).toBe(true)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-split-simple.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("renders multi-hunk split diff", () => {
      function SplitMultiHunk() {
        return (
          <div
            style={{
              width: "100%",
              height: "100%",
              backgroundColor: DIFF_BG,
              overflow: "scroll",
            }}
          >
            <DiffViewer
              hunks={multiHunkPatch}
              filePath="counter.tsx"
              splitView={true}
            />
          </div>
        )
      }

      testRoot.render(<SplitMultiHunk />)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-split-multi.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("scrolling", () => {
    it("scrolls through a long diff and produces different screenshots", () => {
      function LongDiff() {
        // Constrained height so content overflows and scrolling is needed.
        // Full width fills the canvas, but short height forces scroll.
        return (
          <div
            style={{
              width: "100%",
              height: 300,
              backgroundColor: DIFF_BG,
              overflow: "scroll",
            }}
          >
            <DiffViewer
              hunks={longHunk}
              filePath="utils.ts"
              splitView={false}
            />
          </div>
        )
      }

      testRoot.render(<LongDiff />)

      const pathBefore = `${SCREENSHOT_DIR}/gpuix-diff-scroll-before.png`
      const pathAfter = `${SCREENSHOT_DIR}/gpuix-diff-scroll-after.png`
      if (fs.existsSync(pathBefore)) fs.unlinkSync(pathBefore)
      if (fs.existsSync(pathAfter)) fs.unlinkSync(pathAfter)

      // Screenshot before scrolling
      testRoot.renderer.captureScreenshot(pathBefore)

      // Scroll down 200px inside the container
      testRoot.renderer.nativeSimulateScrollWheel(450, 150, 0, -200)

      // Screenshot after scrolling
      testRoot.renderer.captureScreenshot(pathAfter)

      // Both should exist with content
      expect(fs.existsSync(pathBefore)).toBe(true)
      expect(fs.existsSync(pathAfter)).toBe(true)
      expect(fs.statSync(pathBefore).size).toBeGreaterThan(0)
      expect(fs.statSync(pathAfter).size).toBeGreaterThan(0)

      // Before and after scroll should produce different pixels
      expect(
        fs.readFileSync(pathBefore).equals(fs.readFileSync(pathAfter)),
      ).toBe(false)
    })
  })

  describe("empty state", () => {
    it("renders 'No changes' for empty hunks", () => {
      function EmptyDiff() {
        return (
          <div
            style={{
              width: "100%",
              height: "100%",
              backgroundColor: DIFF_BG,
            }}
          >
            <DiffViewer hunks={[]} filePath="empty.ts" />
          </div>
        )
      }

      testRoot.render(<EmptyDiff />)

      const allText = testRoot.renderer.getAllText()
      expect(allText.some((t: string) => t.includes("No changes"))).toBe(true)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-empty.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
    })
  })
})
