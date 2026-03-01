/// Tests for GPUIX event handling — verifies that React components receive
/// events correctly through the full native GPUI pipeline (end-to-end).
///
/// Every test goes through: React render → Rust RetainedTree → GpuixView::render()
/// → build_element() → GPUI layout → native simulate → GPUI hit test/dispatch →
/// event handler → emit_event_full → drainEvents → handleGpuixEvent → React handler.
///
/// All components use explicit sizes so GPUI can lay them out and hit-test
/// against known coordinates.
///
/// @ts-nocheck — JSX types resolve to DOM's HTMLDivElement, not GPUIX's Props.
// @ts-nocheck

import fs from "fs"
import { describe, it, expect, beforeEach } from "vitest"
import React, { useState } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../testing"
import type { EventPayload } from "@gpuix/native"

// All tests require the native GPUI test renderer (cargo build with test-support).
const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("events", () => {
  let testRoot: ReturnType<typeof createTestRoot>

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  describe("click events", () => {
    it("should handle onClick and trigger re-render", () => {
      function Counter() {
        const [count, setCount] = useState(0)
        return (
          <div
            style={{ width: 200, height: 50 }}
            onClick={() => setCount((c) => c + 1)}
          >
            <text>{`Count: ${count}`}</text>
          </div>
        )
      }

      testRoot.render(<Counter />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Count: 0",
        ]
      `)

      // Click within the div bounds (GPUI does hit testing)
      testRoot.renderer.nativeSimulateClick(10, 10)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Count: 1",
        ]
      `)

      // Click again
      testRoot.renderer.nativeSimulateClick(10, 10)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Count: 2",
        ]
      `)
    })
  })

  describe("keyboard events", () => {
    it("should handle onKeyDown and update state", () => {
      function KeyTracker() {
        const [lastKey, setLastKey] = useState("none")
        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => setLastKey(e.key ?? "unknown")}
          >
            <text>{`Key: ${lastKey}`}</text>
          </div>
        )
      }

      testRoot.render(<KeyTracker />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Key: none",
        ]
      `)

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      // GPUI uses "down" not "arrowDown"
      testRoot.renderer.nativeSimulateKeystrokes(div.id, "down")

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Key: down",
        ]
      `)

      testRoot.renderer.nativeSimulateKeystrokes(div.id, "escape")

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Key: escape",
        ]
      `)
    })

    it("should pass modifiers in keyboard events", () => {
      const receivedEvents: EventPayload[] = []

      function ModifierTracker() {
        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<ModifierTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      testRoot.renderer.nativeSimulateKeystrokes(div.id, "cmd-s")

      expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
      const event = receivedEvents.find((e) => e.key === "s")
      expect(event).toBeDefined()
      expect(event!.modifiers?.cmd).toBe(true)
    })
  })

  describe("hover events", () => {
    it("should handle mouseEnter and mouseLeave via mouse move", () => {
      function HoverBox() {
        const [hovered, setHovered] = useState(false)
        return (
          <div
            style={{ width: 200, height: 100 }}
            onMouseEnter={() => setHovered(true)}
            onMouseLeave={() => setHovered(false)}
          >
            <text>{hovered ? "hovered" : "not hovered"}</text>
          </div>
        )
      }

      testRoot.render(<HoverBox />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "not hovered",
        ]
      `)

      // Move mouse into element bounds → triggers on_hover(true) → mouseEnter
      testRoot.renderer.nativeSimulateMouseMove(50, 50)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "hovered",
        ]
      `)

      // Move mouse out of element bounds → triggers on_hover(false) → mouseLeave
      testRoot.renderer.nativeSimulateMouseMove(500, 500)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "not hovered",
        ]
      `)
    })
  })

  describe("mouseDownOutside", () => {
    it("should handle click outside to close pattern", () => {
      function Dropdown() {
        const [open, setOpen] = useState(false)
        return (
          <div style={{ width: 400, height: 400 }}>
            <div
              style={{ width: 100, height: 30 }}
              onClick={() => setOpen(true)}
            >
              <text>trigger</text>
            </div>
            {open && (
              <div
                style={{ width: 100, height: 100 }}
                onMouseDownOutside={() => setOpen(false)}
              >
                <text>dropdown content</text>
              </div>
            )}
          </div>
        )
      }

      testRoot.render(<Dropdown />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "trigger",
        ]
      `)

      // Click on the trigger to open (within trigger bounds)
      testRoot.renderer.nativeSimulateClick(10, 10)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "trigger",
          "dropdown content",
        ]
      `)

      // Click outside the dropdown — GPUI fires on_mouse_down_out
      testRoot.renderer.nativeSimulateClick(350, 350)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "trigger",
        ]
      `)
    })
  })

  describe("keyboard navigation", () => {
    it("should support arrow key navigation in a list", () => {
      function SelectableList() {
        const items = ["Apple", "Banana", "Cherry"]
        const [selected, setSelected] = useState(0)

        return (
          <div
            style={{ width: 200, height: 200 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => {
              if (e.key === "down") {
                setSelected((s) => Math.min(s + 1, items.length - 1))
              } else if (e.key === "up") {
                setSelected((s) => Math.max(s - 1, 0))
              }
            }}
          >
            {items.map((item, i) => (
              <div key={item}>
                <text>{`${i === selected ? "> " : "  "}${item}`}</text>
              </div>
            ))}
          </div>
        )
      }

      testRoot.render(<SelectableList />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "> Apple",
          "  Banana",
          "  Cherry",
        ]
      `)

      const list = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      // Arrow down through native GPUI pipeline
      testRoot.renderer.nativeSimulateKeystrokes(list.id, "down")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "> Banana",
          "  Cherry",
        ]
      `)

      // Arrow down again
      testRoot.renderer.nativeSimulateKeystrokes(list.id, "down")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "  Banana",
          "> Cherry",
        ]
      `)

      // Arrow down at bottom — should stay
      testRoot.renderer.nativeSimulateKeystrokes(list.id, "down")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "  Banana",
          "> Cherry",
        ]
      `)

      // Arrow up
      testRoot.renderer.nativeSimulateKeystrokes(list.id, "up")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "> Banana",
          "  Cherry",
        ]
      `)
    })
  })

  describe("scroll events", () => {
    it("should handle onScroll and receive exact delta values", () => {
      const receivedEvents: EventPayload[] = []

      function ScrollBox() {
        return (
          <div
            style={{ width: 200, height: 200 }}
            onScroll={(e: EventPayload) => receivedEvents.push(e)}
          >
            <text>scrollable</text>
          </div>
        )
      }

      testRoot.render(<ScrollBox />)
      testRoot.renderer.nativeSimulateScrollWheel(100, 100, 0, -50)

      expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
      const scrollEvent = receivedEvents.find(
        (e) => e.eventType === "scroll"
      )
      expect(scrollEvent).toBeDefined()
      expect(scrollEvent!.eventType).toBe("scroll")
      expect(scrollEvent!.deltaX).toBe(0)
      expect(scrollEvent!.deltaY).toBe(-50)
      expect(scrollEvent!.touchPhase).toBe("moved")
    })

    it("should update state on scroll", () => {
      function ScrollCounter() {
        const [scrollCount, setScrollCount] = useState(0)
        return (
          <div
            style={{ width: 200, height: 200 }}
            onScroll={() => setScrollCount((c) => c + 1)}
          >
            <text>{`Scrolls: ${scrollCount}`}</text>
          </div>
        )
      }

      testRoot.render(<ScrollCounter />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Scrolls: 0",
        ]
      `)

      testRoot.renderer.nativeSimulateScrollWheel(100, 100, 0, -30)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Scrolls: 1",
        ]
      `)
    })
  })

  describe("keyDown and keyUp events", () => {
    it("should handle onKeyDown via nativeSimulateKeyDown", () => {
      function KeyTracker() {
        const [lastKey, setLastKey] = useState("none")
        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => setLastKey(e.key ?? "unknown")}
          >
            <text>{`Key: ${lastKey}`}</text>
          </div>
        )
      }

      testRoot.render(<KeyTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      testRoot.renderer.nativeSimulateKeyDown(div.id, "a")

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Key: a",
        ]
      `)
    })

    it("should handle onKeyUp via nativeSimulateKeyUp", () => {
      const events: string[] = []

      function KeyUpTracker() {
        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => events.push(`down:${e.key}`)}
            onKeyUp={(e: EventPayload) => events.push(`up:${e.key}`)}
          />
        )
      }

      testRoot.render(<KeyUpTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown") && d.events.has("keyUp"))!

      testRoot.renderer.nativeSimulateKeyDown(div.id, "enter")
      testRoot.renderer.nativeSimulateKeyUp(div.id, "enter")

      expect(events).toContain("down:enter")
      expect(events).toContain("up:enter")
    })

    it("should handle onKeyUp state update", () => {
      function KeyUpStateTracker() {
        const [lastKey, setLastKey] = useState("none")
        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyUp={(e: EventPayload) => setLastKey(e.key ?? "unknown")}
          >
            <text>{`Released: ${lastKey}`}</text>
          </div>
        )
      }

      testRoot.render(<KeyUpStateTracker />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Released: none",
        ]
      `)

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyUp"))!

      testRoot.renderer.nativeSimulateKeyUp(div.id, "a")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Released: a",
        ]
      `)
    })
  })

  describe("mouseDown and mouseUp events", () => {
    it("should handle onMouseDown and onMouseUp", () => {
      function PressTracker() {
        const [pressed, setPressed] = useState(false)
        return (
          <div
            style={{ width: 200, height: 100 }}
            onMouseDown={() => setPressed(true)}
            onMouseUp={() => setPressed(false)}
          >
            <text>{pressed ? "pressed" : "released"}</text>
          </div>
        )
      }

      testRoot.render(<PressTracker />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "released",
        ]
      `)

      testRoot.renderer.nativeSimulateMouseDown(10, 10)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "pressed",
        ]
      `)

      testRoot.renderer.nativeSimulateMouseUp(10, 10)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "released",
        ]
      `)
    })

    it("should receive correct mouse button in mouseDown payload", () => {
      const receivedEvents: EventPayload[] = []

      function ButtonTracker() {
        return (
          <div
            style={{ width: 200, height: 100 }}
            onMouseDown={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<ButtonTracker />)

      // Left click (button=0)
      testRoot.renderer.nativeSimulateMouseDown(10, 10, 0)
      expect(receivedEvents[0].button).toBe(0)

      // Right click (button=2)
      testRoot.renderer.nativeSimulateMouseDown(10, 10, 2)
      expect(receivedEvents[1].button).toBe(2)

      // Middle click (button=1)
      testRoot.renderer.nativeSimulateMouseDown(10, 10, 1)
      expect(receivedEvents[2].button).toBe(1)
    })
  })

  describe("mouseMove events", () => {
    it("should handle onMouseMove and receive exact position", () => {
      const receivedEvents: EventPayload[] = []

      function MoveTracker() {
        return (
          <div
            style={{ width: 300, height: 300 }}
            onMouseMove={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<MoveTracker />)
      testRoot.renderer.nativeSimulateMouseMove(50, 75)

      expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
      const moveEvent = receivedEvents.find(
        (e) => e.eventType === "mouseMove"
      )
      expect(moveEvent).toBeDefined()
      expect(moveEvent!.eventType).toBe("mouseMove")
      expect(moveEvent!.x).toBe(50)
      expect(moveEvent!.y).toBe(75)
    })

    it("should receive pressedButton during drag", () => {
      const receivedEvents: EventPayload[] = []

      function DragTracker() {
        return (
          <div
            style={{ width: 300, height: 300 }}
            onMouseMove={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<DragTracker />)

      // Move without button pressed
      testRoot.renderer.nativeSimulateMouseMove(10, 10)
      expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
      const noButtonEvent = receivedEvents.find((e) => e.eventType === "mouseMove")!
      expect(noButtonEvent.pressedButton).toBeUndefined()

      // Move with left button pressed (simulating drag)
      receivedEvents.length = 0
      testRoot.renderer.nativeSimulateMouseMove(50, 50, 0)
      const dragEvent = receivedEvents.find((e) => e.eventType === "mouseMove")!
      expect(dragEvent.pressedButton).toBe(0)
    })

    it("should update state on mouse move", () => {
      function PositionTracker() {
        const [pos, setPos] = useState("0,0")
        return (
          <div
            style={{ width: 300, height: 300 }}
            onMouseMove={(e: EventPayload) =>
              setPos(`${Math.round(e.x ?? 0)},${Math.round(e.y ?? 0)}`)
            }
          >
            <text>{`Position: ${pos}`}</text>
          </div>
        )
      }

      testRoot.render(<PositionTracker />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Position: 0,0",
        ]
      `)

      testRoot.renderer.nativeSimulateMouseMove(42, 99)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Position: 42,99",
        ]
      `)
    })
  })

  describe("combined event interactions", () => {
    it("should support keyboard shortcuts with modifiers", () => {
      function ShortcutHandler() {
        const [action, setAction] = useState("none")

        return (
          <div
            style={{ width: 200, height: 50 }}
            tabIndex={0}
            onKeyDown={(e: EventPayload) => {
              const mods = e.modifiers
              if (mods?.cmd && e.key === "s") {
                setAction("save")
              } else if (mods?.cmd && mods?.shift && e.key === "p") {
                setAction("command-palette")
              } else if (e.key === "escape") {
                setAction("cancel")
              }
            }}
          >
            <text>{`Action: ${action}`}</text>
          </div>
        )
      }

      testRoot.render(<ShortcutHandler />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: none",
        ]
      `)

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      // Cmd+S
      testRoot.renderer.nativeSimulateKeystrokes(div.id, "cmd-s")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: save",
        ]
      `)

      // Cmd+Shift+P
      testRoot.renderer.nativeSimulateKeystrokes(div.id, "cmd-shift-p")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: command-palette",
        ]
      `)

      // Escape (no modifiers)
      testRoot.renderer.nativeSimulateKeystrokes(div.id, "escape")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: cancel",
        ]
      `)
    })
  })

  describe("screenshot", () => {
    it("should capture screenshot after interaction", () => {
      function Counter() {
        const [count, setCount] = useState(0)
        return (
          <div
            style={{
              width: 200,
              height: 50,
              backgroundColor: "#1e1e2e",
            }}
            onClick={() => setCount((c) => c + 1)}
          >
            <text style={{ color: "#cdd6f4", fontSize: 14 }}>
              {`Count: ${count}`}
            </text>
          </div>
        )
      }

      testRoot.render(<Counter />)

      // Capture initial state
      const path0 = "/tmp/gpuix-counter-0.png"
      const path1 = "/tmp/gpuix-counter-1.png"

      // Clean up from previous runs
      if (fs.existsSync(path0)) fs.unlinkSync(path0)
      if (fs.existsSync(path1)) fs.unlinkSync(path1)

      testRoot.renderer.captureScreenshot(path0)

      // Click and capture again
      testRoot.renderer.nativeSimulateClick(10, 10)
      testRoot.renderer.captureScreenshot(path1)

      // Verify files exist and have non-zero size
      expect(fs.existsSync(path0)).toBe(true)
      expect(fs.existsSync(path1)).toBe(true)
      expect(fs.statSync(path0).size).toBeGreaterThan(0)
      expect(fs.statSync(path1).size).toBeGreaterThan(0)
    })
  })

  describe("tree structure", () => {
    it("should produce correct element tree", () => {
      function App() {
        return (
          <div style={{ display: "flex", gap: 8 }}>
            <text>Hello</text>
            <div onClick={() => {}}>
              <text>Click me</text>
            </div>
          </div>
        )
      }

      testRoot.render(<App />)
      expect(testRoot.renderer.toJSON()).toMatchInlineSnapshot(`
        {
          "children": [
            {
              "children": [
                {
                  "id": 1,
                  "text": "Hello",
                  "type": "text",
                },
              ],
              "id": 2,
              "type": "text",
            },
            {
              "children": [
                {
                  "children": [
                    {
                      "id": 3,
                      "text": "Click me",
                      "type": "text",
                    },
                  ],
                  "id": 4,
                  "type": "text",
                },
              ],
              "events": [
                "click",
              ],
              "id": 5,
              "type": "div",
            },
          ],
          "id": 6,
          "style": {
            "display": "flex",
            "gap": 8,
          },
          "type": "div",
        }
      `)
    })
  })
})
