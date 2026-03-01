/// Tests for GPUIX custom <input> element — validates the polymorphic
/// CustomElement trait pipeline end-to-end:
///
///   React render → host-config → setCustomProp (napi) → RetainedTree →
///   CustomElementRegistry → InputElement::set_prop/render → GPUI layout →
///   native simulate → event handler → emit_event_full → React handler
///
/// All tests use the native GPUI test renderer (real Metal rendering).
// @ts-nocheck

import fs from "fs"
import { describe, it, expect, beforeEach } from "vitest"
import React, { useState } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../testing"
import type { EventPayload } from "@gpuix/native"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("custom element: input", () => {
  let testRoot: ReturnType<typeof createTestRoot>

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  describe("rendering", () => {
    it("should render input with value prop", () => {
      function App() {
        return (
          <div style={{ width: 400, height: 100 }}>
            <input
              value="hello world"
              style={{ width: 300, height: 40 }}
            />
          </div>
        )
      }

      testRoot.render(<App />)
      // The input element should be created as type "input" in the tree
      const inputs = testRoot.renderer.findByType("input")
      expect(inputs.length).toBe(1)
    })

    it("should render placeholder when value is empty", () => {
      function App() {
        return (
          <div style={{ width: 400, height: 100 }}>
            <input
              value=""
              placeholder="Type here..."
              style={{ width: 300, height: 40 }}
            />
          </div>
        )
      }

      testRoot.render(<App />)
      const inputs = testRoot.renderer.findByType("input")
      expect(inputs.length).toBe(1)
    })
  })

  describe("keyboard events", () => {
    it("should handle onKeyDown and build text from keystrokes", () => {
      function TextInput() {
        const [text, setText] = useState("")
        return (
          <div style={{ width: 400, height: 100 }}>
            <input
              value={text}
              placeholder="Type here..."
              style={{ width: 300, height: 40 }}
              onKeyDown={(e: EventPayload) => {
                if (e.key === "backspace") {
                  setText((t) => t.slice(0, -1))
                } else if (e.keyChar) {
                  setText((t) => t + e.keyChar)
                }
              }}
            />
            <text>{`Value: ${text}`}</text>
          </div>
        )
      }

      testRoot.render(<TextInput />)

      // Find the input element (it has keyDown event)
      const input = testRoot.renderer
        .findByType("input")
        .find((el) => el.events.has("keyDown"))!
      expect(input).toBeDefined()

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Value: ",
        ]
      `)

      // Type "hi" via native GPUI keystrokes
      testRoot.renderer.nativeSimulateKeystrokes(input.id, "h")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Value: h",
        ]
      `)

      testRoot.renderer.nativeSimulateKeystrokes(input.id, "i")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Value: hi",
        ]
      `)
    })

    it("should handle backspace", () => {
      function TextInput() {
        const [text, setText] = useState("abc")
        return (
          <div style={{ width: 400, height: 100 }}>
            <input
              value={text}
              style={{ width: 300, height: 40 }}
              onKeyDown={(e: EventPayload) => {
                if (e.key === "backspace") {
                  setText((t) => t.slice(0, -1))
                } else if (e.keyChar) {
                  setText((t) => t + e.keyChar)
                }
              }}
            />
            <text>{`Value: ${text}`}</text>
          </div>
        )
      }

      testRoot.render(<TextInput />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Value: abc",
        ]
      `)

      const input = testRoot.renderer
        .findByType("input")
        .find((el) => el.events.has("keyDown"))!

      testRoot.renderer.nativeSimulateKeystrokes(input.id, "backspace")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Value: ab",
        ]
      `)
    })
  })

  describe("screenshots", () => {
    it("should capture screenshot of input with text", () => {
      function InputScreenshotProbe() {
        const [text, setText] = useState("")
        return (
          <div
            style={{
              display: "flex",
              alignItems: "center",
              justifyContent: "center",
              width: "100%",
              height: "100%",
              backgroundColor: "#0f111a",
            }}
          >
            <div
              style={{
                width: 350,
                display: "flex",
                flexDirection: "column",
                gap: 12,
                padding: 24,
                borderRadius: 16,
                backgroundColor: "#1a1d2e",
              }}
            >
              <text style={{ color: "#a8b2d8", fontSize: 14 }}>
                Custom Input Element
              </text>
              <input
                value={text}
                placeholder="Type something..."
                style={{ width: "100%", height: 36 }}
                onKeyDown={(e: EventPayload) => {
                  if (e.key === "backspace") {
                    setText((t) => t.slice(0, -1))
                  } else if (e.keyChar) {
                    setText((t) => t + e.keyChar)
                  }
                }}
              />
              <text style={{ color: "#7f8bb3", fontSize: 12 }}>
                {text ? `You typed: ${text}` : "Start typing..."}
              </text>
            </div>
          </div>
        )
      }

      testRoot.render(<InputScreenshotProbe />)

      const path0 = "/tmp/gpuix-input-0.png"
      const path1 = "/tmp/gpuix-input-1.png"

      if (fs.existsSync(path0)) fs.unlinkSync(path0)
      if (fs.existsSync(path1)) fs.unlinkSync(path1)

      // Screenshot empty input (shows placeholder)
      testRoot.renderer.captureScreenshot(path0)

      // Type some text
      const input = testRoot.renderer
        .findByType("input")
        .find((el) => el.events.has("keyDown"))!

      testRoot.renderer.nativeSimulateKeystrokes(input.id, "H")
      testRoot.renderer.nativeSimulateKeystrokes(input.id, "e")
      testRoot.renderer.nativeSimulateKeystrokes(input.id, "l")
      testRoot.renderer.nativeSimulateKeystrokes(input.id, "l")
      testRoot.renderer.nativeSimulateKeystrokes(input.id, "o")

      // Screenshot input with text
      testRoot.renderer.captureScreenshot(path1)

      // Both should exist and differ
      expect(fs.existsSync(path0)).toBe(true)
      expect(fs.existsSync(path1)).toBe(true)
      expect(fs.statSync(path0).size).toBeGreaterThan(0)
      expect(fs.statSync(path1).size).toBeGreaterThan(0)

      const before = fs.readFileSync(path0)
      const after = fs.readFileSync(path1)
      expect(before.equals(after)).toBe(false)
    })
  })

  describe("tree structure", () => {
    it("should produce correct element tree with custom type", () => {
      function App() {
        return (
          <div style={{ display: "flex", gap: 8 }}>
            <input value="test" placeholder="..." />
            <text>Label</text>
          </div>
        )
      }

      testRoot.render(<App />)
      const json = testRoot.renderer.toJSON() as any
      expect(json.type).toBe("div")

      // Find the input child
      const inputChild = json.children?.find(
        (c: any) => c.type === "input"
      )
      expect(inputChild).toBeDefined()
      expect(inputChild.type).toBe("input")
    })
  })
})
