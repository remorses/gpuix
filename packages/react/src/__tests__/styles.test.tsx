/// Tests for GPUIX style properties added for diff-viewer support.
///
/// Validates: alignSelf, flexShrink/flexGrow values, fontFamily, fontWeight,
/// text backgroundColor, and flexWrap — all through the full native GPUI
/// pipeline with screenshots for visual validation.
///
/// @ts-nocheck

import fs from "fs"
import { describe, it, expect, beforeEach } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer } from "../testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const SCREENSHOT_DIR = "/tmp"

/** Centering wrapper — fills the test window and centers content. */
function Center({ children }: { children: React.ReactNode }) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        width: "100%",
        height: "100%",
        backgroundColor: "#11111b",
      }}
    >
      {children}
    </div>
  )
}

describeNative("style properties", () => {
  let testRoot: ReturnType<typeof createTestRoot>

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  describe("alignSelf", () => {
    it("should apply alignSelf: stretch to fill cross-axis", () => {
      function AlignSelfTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                width: 400,
                height: 200,
                backgroundColor: "#1e1e2e",
                gap: 8,
                padding: 12,
                borderRadius: 8,
              }}
            >
              {/* Left gutter — should stretch to fill the row height */}
              <div
                style={{
                  alignSelf: "stretch",
                  width: 50,
                  backgroundColor: "#313244",
                  flexShrink: 0,
                }}
              >
                <text style={{ color: "#6c7086", fontSize: 12 }}>01</text>
              </div>
              {/* Content area */}
              <div
                style={{
                  display: "flex",
                  flexDirection: "column",
                  flexGrow: 1,
                  gap: 4,
                }}
              >
                <text style={{ color: "#cdd6f4", fontSize: 14 }}>
                  Line content that may wrap
                </text>
                <text style={{ color: "#a6adc8", fontSize: 12 }}>
                  Second line of content
                </text>
              </div>
            </div>
          </Center>
        )
      }

      testRoot.render(<AlignSelfTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "01",
          "Line content that may wrap",
          "Second line of content",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-align-self.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("flexShrink value", () => {
    it("should respect flexShrink: 0 to prevent shrinking", () => {
      function FlexShrinkTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                width: 300,
                height: 100,
                backgroundColor: "#1e1e2e",
                borderRadius: 8,
              }}
            >
              {/* Fixed-width gutter that must NOT shrink */}
              <div
                style={{
                  width: 60,
                  flexShrink: 0,
                  backgroundColor: "#45475a",
                  padding: 8,
                }}
              >
                <text style={{ color: "#bac2de", fontSize: 12 }}>42</text>
              </div>
              {/* Flexible content that CAN shrink */}
              <div
                style={{
                  flexGrow: 1,
                  flexShrink: 1,
                  padding: 8,
                  backgroundColor: "#313244",
                }}
              >
                <text style={{ color: "#cdd6f4", fontSize: 12 }}>
                  const x = someVeryLongVariableName
                </text>
              </div>
            </div>
          </Center>
        )
      }

      testRoot.render(<FlexShrinkTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "42",
          "const x = someVeryLongVariableName",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-flex-shrink.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("flexGrow value", () => {
    it("should respect flexGrow: 0 vs flexGrow: 1", () => {
      function FlexGrowTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                width: 400,
                height: 80,
                backgroundColor: "#1e1e2e",
                gap: 4,
                borderRadius: 8,
              }}
            >
              <div
                style={{
                  flexGrow: 0,
                  width: 80,
                  backgroundColor: "#f38ba8",
                  padding: 8,
                }}
              >
                <text style={{ color: "#1e1e2e", fontSize: 12 }}>fixed</text>
              </div>
              <div
                style={{
                  flexGrow: 1,
                  backgroundColor: "#a6e3a1",
                  padding: 8,
                }}
              >
                <text style={{ color: "#1e1e2e", fontSize: 12 }}>grows</text>
              </div>
              <div
                style={{
                  flexGrow: 2,
                  backgroundColor: "#89b4fa",
                  padding: 8,
                }}
              >
                <text style={{ color: "#1e1e2e", fontSize: 12 }}>grows 2x</text>
              </div>
            </div>
          </Center>
        )
      }

      testRoot.render(<FlexGrowTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "fixed",
          "grows",
          "grows 2x",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-flex-grow.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("fontFamily", () => {
    it("should render text with a custom monospace font", () => {
      function FontFamilyTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 500,
                height: 160,
                backgroundColor: "#1e1e2e",
                padding: 16,
                gap: 12,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 14 }}>
                Default font: The quick brown fox
              </text>
              <text
                style={{ color: "#cdd6f4", fontSize: 14, fontFamily: "Menlo" }}
              >
                Menlo font: The quick brown fox
              </text>
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 14,
                  fontFamily: "Courier New",
                }}
              >
                Courier: The quick brown fox
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<FontFamilyTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "Default font: The quick brown fox",
          "Menlo font: The quick brown fox",
          "Courier: The quick brown fox",
        ]
      `)

      // Screenshot to verify visual difference between fonts
      const path = `${SCREENSHOT_DIR}/gpuix-font-family.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("should visually differ from default font", () => {
      // Render with default font first
      function DefaultFont() {
        return (
          <Center>
            <div
              style={{
                width: 400,
                height: 60,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 16 }}>
                MMMWWW iiiiii
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<DefaultFont />)
      const pathDefault = `${SCREENSHOT_DIR}/gpuix-font-default.png`
      if (fs.existsSync(pathDefault)) fs.unlinkSync(pathDefault)
      testRoot.renderer.captureScreenshot(pathDefault)

      // Render with monospace font
      function MonoFont() {
        return (
          <Center>
            <div
              style={{
                width: 400,
                height: 60,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 16, fontFamily: "Menlo" }}>
                MMMWWW iiiiii
              </text>
            </div>
          </Center>
        )
      }

      // Need a fresh test root for the second render
      const testRoot2 = createTestRoot()
      testRoot2.render(<MonoFont />)
      const pathMono = `${SCREENSHOT_DIR}/gpuix-font-mono.png`
      if (fs.existsSync(pathMono)) fs.unlinkSync(pathMono)
      testRoot2.renderer.captureScreenshot(pathMono)

      // The two screenshots should differ (different glyph widths)
      const defaultBytes = fs.readFileSync(pathDefault)
      const monoBytes = fs.readFileSync(pathMono)
      expect(defaultBytes.equals(monoBytes)).toBe(false)
    })
  })

  describe("fontWeight", () => {
    it("should render bold text differently from normal", () => {
      function FontWeightTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 400,
                height: 140,
                backgroundColor: "#1e1e2e",
                padding: 16,
                gap: 8,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: "normal" }}>
                Normal weight text
              </text>
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: "bold" }}>
                Bold weight text
              </text>
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: "300" }}>
                Light (300) weight text
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<FontWeightTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "Normal weight text",
          "Bold weight text",
          "Light (300) weight text",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-font-weight.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("should accept numeric fontWeight values (700, 300, etc.)", () => {
      function NumericWeightTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 400,
                height: 180,
                backgroundColor: "#1e1e2e",
                padding: 16,
                gap: 8,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: 400 }}>
                fontWeight: 400 (normal)
              </text>
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: 700 }}>
                fontWeight: 700 (bold)
              </text>
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: 300 }}>
                fontWeight: 300 (light)
              </text>
              <text style={{ color: "#cdd6f4", fontSize: 16, fontWeight: 900 }}>
                fontWeight: 900 (black)
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<NumericWeightTest />)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "fontWeight: 400 (normal)",
          "fontWeight: 700 (bold)",
          "fontWeight: 300 (light)",
          "fontWeight: 900 (black)",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-font-weight-numeric.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("text backgroundColor", () => {
    it("should render individual text tokens with background colors", () => {
      function TextBgTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 500,
                height: 120,
                backgroundColor: "#1e1e2e",
                padding: 12,
                gap: 8,
                borderRadius: 8,
              }}
            >
              {/* Simulated diff line with word-level highlights */}
              <div style={{ display: "flex", flexDirection: "row" }}>
                <text
                  style={{
                    color: "#f38ba8",
                    backgroundColor: "#53222e",
                    fontSize: 13,
                  }}
                >
                  removed
                </text>
                <text style={{ color: "#cdd6f4", fontSize: 13 }}> word </text>
                <text
                  style={{
                    color: "#a6e3a1",
                    backgroundColor: "#1e3a2c",
                    fontSize: 13,
                  }}
                >
                  added
                </text>
              </div>
              {/* Another row with mixed highlights */}
              <div style={{ display: "flex", flexDirection: "row" }}>
                <text style={{ color: "#89b4fa", backgroundColor: "#1e2d4a", fontSize: 13 }}>
                  keyword
                </text>
                <text style={{ color: "#cdd6f4", fontSize: 13 }}> = </text>
                <text style={{ color: "#fab387", backgroundColor: "#3d2a1e", fontSize: 13 }}>
                  "string value"
                </text>
              </div>
            </div>
          </Center>
        )
      }

      testRoot.render(<TextBgTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "removed",
          " word ",
          "added",
          "keyword",
          " = ",
          ""string value"",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-text-bg.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("flexWrap", () => {
    it("should wrap items to next line when container overflows", () => {
      function FlexWrapTest() {
        const items = ["fn", "main", "()", "{", "let", "x", "=", "42", ";", "}"]
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                flexWrap: "wrap",
                width: 200,
                height: 200,
                backgroundColor: "#1e1e2e",
                padding: 8,
                gap: 6,
                borderRadius: 8,
              }}
            >
              {items.map((item, i) => (
                <div
                  key={i}
                  style={{
                    backgroundColor: "#313244",
                    padding: 4,
                    borderRadius: 4,
                  }}
                >
                  <text style={{ color: "#cdd6f4", fontSize: 12 }}>{item}</text>
                </div>
              ))}
            </div>
          </Center>
        )
      }

      testRoot.render(<FlexWrapTest />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "fn",
          "main",
          "()",
          "{",
          "let",
          "x",
          "=",
          "42",
          ";",
          "}",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-flex-wrap.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  describe("composite: diff viewer row", () => {
    it("should render a complete diff-viewer-like row with all features", () => {
      function DiffRow() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 600,
                height: 200,
                backgroundColor: "#1e1e2e",
                fontFamily: "Menlo",
                borderRadius: 8,
              }}
            >
            {/* Removed line */}
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                backgroundColor: "#2d1520",
              }}
            >
              {/* Line number gutter — stretches to fill row, never shrinks */}
              <div
                style={{
                  alignSelf: "stretch",
                  width: 50,
                  flexShrink: 0,
                  backgroundColor: "#351b26",
                  padding: 4,
                }}
              >
                <text style={{ color: "#6c5060", fontSize: 12 }}>12</text>
              </div>
              {/* Code tokens in a flex row */}
              <div
                style={{
                  display: "flex",
                  flexDirection: "row",
                  flexGrow: 1,
                  padding: 4,
                }}
              >
                <text style={{ color: "#cba6f7", fontSize: 12 }}>const </text>
                <text
                  style={{
                    color: "#f38ba8",
                    backgroundColor: "#53222e",
                    fontSize: 12,
                  }}
                >
                  oldName
                </text>
                <text style={{ color: "#cdd6f4", fontSize: 12 }}> = </text>
                <text style={{ color: "#a6e3a1", fontSize: 12 }}>
                  "hello"
                </text>
              </div>
            </div>

            {/* Added line */}
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                backgroundColor: "#152d1b",
              }}
            >
              <div
                style={{
                  alignSelf: "stretch",
                  width: 50,
                  flexShrink: 0,
                  backgroundColor: "#1b3521",
                  padding: 4,
                }}
              >
                <text style={{ color: "#506c55", fontSize: 12 }}>13</text>
              </div>
              <div
                style={{
                  display: "flex",
                  flexDirection: "row",
                  flexGrow: 1,
                  padding: 4,
                }}
              >
                <text style={{ color: "#cba6f7", fontSize: 12 }}>const </text>
                <text
                  style={{
                    color: "#a6e3a1",
                    backgroundColor: "#1e3a2c",
                    fontSize: 12,
                  }}
                >
                  newName
                </text>
                <text style={{ color: "#cdd6f4", fontSize: 12 }}> = </text>
                <text style={{ color: "#a6e3a1", fontSize: 12 }}>
                  "hello"
                </text>
              </div>
            </div>

            {/* Title bar with bold text */}
            <div
              style={{
                display: "flex",
                flexDirection: "row",
                padding: 8,
                backgroundColor: "#313244",
                gap: 8,
              }}
            >
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 13,
                  fontWeight: "bold",
                }}
              >
                src/renderer.rs
              </text>
              <text style={{ color: "#6c7086", fontSize: 13 }}>
                (2 changes)
              </text>
            </div>
          </div>
          </Center>
        )
      }

      testRoot.render(<DiffRow />)

      const texts = testRoot.renderer.getAllText()
      expect(texts).toMatchInlineSnapshot(`
        [
          "12",
          "const ",
          "oldName",
          " = ",
          ""hello"",
          "13",
          "const ",
          "newName",
          " = ",
          ""hello"",
          "src/renderer.rs",
          "(2 changes)",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-diff-row.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  // ── whiteSpace ──────────────────────────────────────────────────

  describe("whiteSpace", () => {
    it("nowrap: long text stays on one line and overflows container", () => {
      function NowrapTest() {
        return (
          <Center>
            <div
              style={{
                width: 300,
                height: 80,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                overflow: "hidden",
              }}
            >
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 13,
                  whiteSpace: "nowrap",
                }}
              >
                This is a very long line of text that should never wrap to a second line regardless of container width
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<NowrapTest />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "This is a very long line of text that should never wrap to a second line regardless of container width",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-whitespace-nowrap.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("normal: text wraps within container width", () => {
      function WrapTest() {
        return (
          <Center>
            <div
              style={{
                width: 200,
                height: 120,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 13,
                  whiteSpace: "normal",
                }}
              >
                This text should wrap naturally within the narrow container width producing multiple lines
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<WrapTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-whitespace-normal.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("nowrap vs normal: screenshots should differ for same text", () => {
      function NowrapBox() {
        return (
          <Center>
            <div
              style={{
                width: 150,
                height: 80,
                backgroundColor: "#1e1e2e",
                padding: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 13, whiteSpace: "nowrap" }}>
                The quick brown fox jumps over
              </text>
            </div>
          </Center>
        )
      }

      function NormalBox() {
        return (
          <Center>
            <div
              style={{
                width: 150,
                height: 80,
                backgroundColor: "#1e1e2e",
                padding: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 13, whiteSpace: "normal" }}>
                The quick brown fox jumps over
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<NowrapBox />)
      const pathNowrap = `${SCREENSHOT_DIR}/gpuix-ws-nowrap-cmp.png`
      if (fs.existsSync(pathNowrap)) fs.unlinkSync(pathNowrap)
      testRoot.renderer.captureScreenshot(pathNowrap)

      const testRoot2 = createTestRoot()
      testRoot2.render(<NormalBox />)
      const pathNormal = `${SCREENSHOT_DIR}/gpuix-ws-normal-cmp.png`
      if (fs.existsSync(pathNormal)) fs.unlinkSync(pathNormal)
      testRoot2.renderer.captureScreenshot(pathNormal)

      const nowrapBytes = fs.readFileSync(pathNowrap)
      const normalBytes = fs.readFileSync(pathNormal)
      expect(nowrapBytes.equals(normalBytes)).toBe(false)
    })

    it("nowrap on div: applies to child text elements via inheritance", () => {
      function DivNowrapTest() {
        return (
          <Center>
            <div
              style={{
                width: 200,
                height: 80,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                whiteSpace: "nowrap",
                overflow: "hidden",
              }}
            >
              <text style={{ color: "#89b4fa", fontSize: 13 }}>
                const longVariableName = calculateSomethingImportant()
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<DivNowrapTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-whitespace-div-nowrap.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  // ── textOverflow ────────────────────────────────────────────────

  describe("textOverflow", () => {
    it("ellipsis: truncates long text with ... at end", () => {
      function EllipsisTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 300,
                height: 100,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                gap: 8,
              }}
            >
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 13,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                packages/native/src/renderer.rs — a very long file path that should be truncated
              </text>
              <text style={{ color: "#6c7086", fontSize: 11 }}>
                textOverflow: ellipsis
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<EllipsisTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-text-overflow-ellipsis.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("ellipsis-start: truncates long text with ... at start", () => {
      function EllipsisStartTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 300,
                height: 100,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                gap: 8,
              }}
            >
              <text
                style={{
                  color: "#cdd6f4",
                  fontSize: 13,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis-start",
                }}
              >
                packages/native/src/renderer.rs — a very long file path that should be truncated from start
              </text>
              <text style={{ color: "#6c7086", fontSize: 11 }}>
                textOverflow: ellipsis-start
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<EllipsisStartTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-text-overflow-start.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("ellipsis vs ellipsis-start: produce different screenshots", () => {
      const longText = "src/packages/native/custom_elements/input.rs — handle keyboard events and focus state"

      function EndEllipsis() {
        return (
          <Center>
            <div style={{ width: 250, height: 50, backgroundColor: "#1e1e2e", padding: 8 }}>
              <text style={{
                color: "#cdd6f4", fontSize: 13,
                whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis",
              }}>
                {longText}
              </text>
            </div>
          </Center>
        )
      }

      function StartEllipsis() {
        return (
          <Center>
            <div style={{ width: 250, height: 50, backgroundColor: "#1e1e2e", padding: 8 }}>
              <text style={{
                color: "#cdd6f4", fontSize: 13,
                whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis-start",
              }}>
                {longText}
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<EndEllipsis />)
      const pathEnd = `${SCREENSHOT_DIR}/gpuix-ellipsis-end-cmp.png`
      if (fs.existsSync(pathEnd)) fs.unlinkSync(pathEnd)
      testRoot.renderer.captureScreenshot(pathEnd)

      const testRoot2 = createTestRoot()
      testRoot2.render(<StartEllipsis />)
      const pathStart = `${SCREENSHOT_DIR}/gpuix-ellipsis-start-cmp.png`
      if (fs.existsSync(pathStart)) fs.unlinkSync(pathStart)
      testRoot2.renderer.captureScreenshot(pathStart)

      const endBytes = fs.readFileSync(pathEnd)
      const startBytes = fs.readFileSync(pathStart)
      expect(endBytes.equals(startBytes)).toBe(false)
    })

    it("short text: no truncation when text fits", () => {
      function ShortTextTest() {
        return (
          <Center>
            <div
              style={{
                width: 400,
                height: 60,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <text
                style={{
                  color: "#a6e3a1",
                  fontSize: 13,
                  whiteSpace: "nowrap",
                  overflow: "hidden",
                  textOverflow: "ellipsis",
                }}
              >
                short.ts
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<ShortTextTest />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "short.ts",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-text-overflow-short.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
    })
  })

  // ── lineClamp ───────────────────────────────────────────────────

  describe("lineClamp", () => {
    it("clamps multi-line text to specified number of lines", () => {
      function LineClampTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 300,
                height: 200,
                backgroundColor: "#1e1e2e",
                padding: 16,
                borderRadius: 8,
                gap: 12,
              }}
            >
              <text style={{ color: "#6c7086", fontSize: 11 }}>
                lineClamp: 2
              </text>
              <div
                style={{
                  width: 260,
                  backgroundColor: "#313244",
                  padding: 8,
                  borderRadius: 4,
                }}
              >
                <text style={{ color: "#cdd6f4", fontSize: 13, lineClamp: 2 }}>
                  This is a paragraph of text that should be clamped to exactly two lines. Any content beyond the second line will be hidden and not visible in the rendered output. It keeps going and going.
                </text>
              </div>
              <text style={{ color: "#6c7086", fontSize: 11 }}>
                lineClamp: 1
              </text>
              <div
                style={{
                  width: 260,
                  backgroundColor: "#313244",
                  padding: 8,
                  borderRadius: 4,
                }}
              >
                <text style={{ color: "#cdd6f4", fontSize: 13, lineClamp: 1 }}>
                  Single line clamp — this text should be limited to one visible line only, truncating the rest.
                </text>
              </div>
            </div>
          </Center>
        )
      }

      testRoot.render(<LineClampTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-line-clamp.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("lineClamp on div: applies to child text elements", () => {
      function DivClampTest() {
        return (
          <Center>
            <div
              style={{
                width: 250,
                height: 100,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                lineClamp: 3,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 13 }}>
                Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<DivClampTest />)

      const path = `${SCREENSHOT_DIR}/gpuix-line-clamp-div.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })

    it("lineClamp 1 vs 3: produce different screenshots", () => {
      const longText = "A long paragraph that will need multiple lines to display fully when rendered at a reasonable font size inside a narrow container element."

      function Clamp1() {
        return (
          <Center>
            <div style={{ width: 200, height: 80, backgroundColor: "#1e1e2e", padding: 8 }}>
              <text style={{ color: "#cdd6f4", fontSize: 13, lineClamp: 1 }}>
                {longText}
              </text>
            </div>
          </Center>
        )
      }

      function Clamp3() {
        return (
          <Center>
            <div style={{ width: 200, height: 80, backgroundColor: "#1e1e2e", padding: 8 }}>
              <text style={{ color: "#cdd6f4", fontSize: 13, lineClamp: 3 }}>
                {longText}
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<Clamp1 />)
      const path1 = `${SCREENSHOT_DIR}/gpuix-clamp1-cmp.png`
      if (fs.existsSync(path1)) fs.unlinkSync(path1)
      testRoot.renderer.captureScreenshot(path1)

      const testRoot2 = createTestRoot()
      testRoot2.render(<Clamp3 />)
      const path3 = `${SCREENSHOT_DIR}/gpuix-clamp3-cmp.png`
      if (fs.existsSync(path3)) fs.unlinkSync(path3)
      testRoot2.renderer.captureScreenshot(path3)

      const bytes1 = fs.readFileSync(path1)
      const bytes3 = fs.readFileSync(path3)
      expect(bytes1.equals(bytes3)).toBe(false)
    })

    it("edge case: lineClamp 0 is ignored (no clamping)", () => {
      function Clamp0Test() {
        return (
          <Center>
            <div
              style={{
                width: 200,
                height: 120,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
              }}
            >
              <text style={{ color: "#cdd6f4", fontSize: 13, lineClamp: 0 }}>
                lineClamp zero should not clamp. This text should wrap and show fully without any line limit applied.
              </text>
            </div>
          </Center>
        )
      }

      testRoot.render(<Clamp0Test />)

      const path = `${SCREENSHOT_DIR}/gpuix-line-clamp-zero.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })

  // ── pre-like behavior composite ─────────────────────────────────

  describe("pre-like behavior", () => {
    it("simulates white-space: pre by splitting lines with nowrap", () => {
      const code = [
        "fn main() {",
        "    let x = 42;",
        "    println!(\"{}\", x);",
        "}",
      ]
      function PreTest() {
        return (
          <Center>
            <div
              style={{
                display: "flex",
                flexDirection: "column",
                width: 400,
                height: 160,
                backgroundColor: "#1e1e2e",
                padding: 12,
                borderRadius: 8,
                fontFamily: "Menlo",
              }}
            >
              {code.map((line, i) => (
                <text
                  key={i}
                  style={{
                    color: "#cdd6f4",
                    fontSize: 13,
                    whiteSpace: "nowrap",
                  }}
                >
                  {line}
                </text>
              ))}
            </div>
          </Center>
        )
      }

      testRoot.render(<PreTest />)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "fn main() {",
          "    let x = 42;",
          "    println!("{}", x);",
          "}",
        ]
      `)

      const path = `${SCREENSHOT_DIR}/gpuix-pre-like.png`
      if (fs.existsSync(path)) fs.unlinkSync(path)
      testRoot.renderer.captureScreenshot(path)
      expect(fs.existsSync(path)).toBe(true)
      expect(fs.statSync(path).size).toBeGreaterThan(0)
    })
  })
})
