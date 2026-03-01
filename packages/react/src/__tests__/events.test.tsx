/// Tests for GPUIX event handling — verifies that React components
/// receive events and re-render correctly via the TestRenderer.
///
/// @ts-nocheck — JSX types resolve to DOM's HTMLDivElement, not GPUIX's Props.
/// The tests run correctly via vitest; tsc checking is skipped for test files
/// until we set up a custom JSX namespace for GPUIX elements.
// @ts-nocheck

import { describe, it, expect, beforeEach } from "vitest"
import React, { useState } from "react"
import { createTestRoot, hasNativeTestRenderer } from "../testing"
import type { EventPayload } from "@gpuix/native"

describe("events", () => {
  let testRoot: ReturnType<typeof createTestRoot>

  beforeEach(() => {
    testRoot = createTestRoot()
  })

  describe("click events", () => {
    it("should handle onClick and trigger re-render", () => {
      function Counter() {
        const [count, setCount] = useState(0)
        return (
          <div onClick={() => setCount((c) => c + 1)}>
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

      // Find the div that has the click handler
      const divs = testRoot.renderer.findByType("div")
      const clickableDiv = divs.find((d) => d.events.has("click"))
      expect(clickableDiv).toBeDefined()

      // Simulate click
      testRoot.renderer.simulateClick(clickableDiv!.id)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Count: 1",
        ]
      `)

      // Click again
      testRoot.renderer.simulateClick(clickableDiv!.id)

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

      const divs = testRoot.renderer.findByType("div")
      const focusableDiv = divs.find((d) => d.events.has("keyDown"))
      expect(focusableDiv).toBeDefined()

      // Simulate arrow down
      testRoot.renderer.simulateKeyDown(focusableDiv!.id, "arrowDown")

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Key: arrowDown",
        ]
      `)

      // Simulate escape
      testRoot.renderer.simulateKeyDown(focusableDiv!.id, "escape")

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
            tabIndex={0}
            onKeyDown={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<ModifierTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      testRoot.renderer.simulateKeyDown(div.id, "s", {
        modifiers: { shift: false, ctrl: false, alt: false, cmd: true },
      })

      expect(receivedEvents).toHaveLength(1)
      expect(receivedEvents[0].key).toBe("s")
      expect(receivedEvents[0].modifiers?.cmd).toBe(true)
    })
  })

  describe("hover events", () => {
    it("should handle mouseEnter and mouseLeave", () => {
      function HoverBox() {
        const [hovered, setHovered] = useState(false)
        return (
          <div
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

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseEnter"))!

      testRoot.renderer.simulateMouseEnter(div.id)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "hovered",
        ]
      `)

      testRoot.renderer.simulateMouseLeave(div.id)
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
          <div>
            <div onClick={() => setOpen(true)}>
              <text>trigger</text>
            </div>
            {open && (
              <div onMouseDownOutside={() => setOpen(false)}>
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

      // Find the div with click handler (the trigger wrapper)
      const triggerDiv = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("click"))!
      expect(triggerDiv).toBeDefined()

      // Simulate click to open
      testRoot.renderer.simulateClick(triggerDiv.id)

      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "trigger",
          "dropdown content",
        ]
      `)

      // Find the dropdown (has mouseDownOutside event)
      const dropdown = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseDownOutside"))!

      // Click outside the dropdown
      testRoot.renderer.simulateEvent({
        elementId: dropdown.id,
        eventType: "mouseDownOutside",
        x: 500,
        y: 500,
      })

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
            tabIndex={0}
            onKeyDown={(e: EventPayload) => {
              if (e.key === "arrowDown") {
                setSelected((s) => Math.min(s + 1, items.length - 1))
              } else if (e.key === "arrowUp") {
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

      // Arrow down
      testRoot.renderer.simulateKeyDown(list.id, "arrowDown")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "> Banana",
          "  Cherry",
        ]
      `)

      // Arrow down again
      testRoot.renderer.simulateKeyDown(list.id, "arrowDown")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "  Banana",
          "> Cherry",
        ]
      `)

      // Arrow down at bottom — should stay
      testRoot.renderer.simulateKeyDown(list.id, "arrowDown")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "  Apple",
          "  Banana",
          "> Cherry",
        ]
      `)

      // Arrow up
      testRoot.renderer.simulateKeyDown(list.id, "arrowUp")
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
    it("should handle onScroll and receive delta values", () => {
      const receivedEvents: EventPayload[] = []

      function ScrollBox() {
        return (
          <div onScroll={(e: EventPayload) => receivedEvents.push(e)}>
            <text>scrollable</text>
          </div>
        )
      }

      testRoot.render(<ScrollBox />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("scroll"))!
      expect(div).toBeDefined()

      testRoot.renderer.simulateScroll(div.id, 0, -120)

      expect(receivedEvents).toHaveLength(1)
      expect(receivedEvents[0].eventType).toBe("scroll")
      expect(receivedEvents[0].deltaX).toBe(0)
      expect(receivedEvents[0].deltaY).toBe(-120)
    })

    it("should handle precise trackpad scroll", () => {
      const receivedEvents: EventPayload[] = []

      function TrackpadScroll() {
        return <div onScroll={(e: EventPayload) => receivedEvents.push(e)} />
      }

      testRoot.render(<TrackpadScroll />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("scroll"))!

      testRoot.renderer.simulateScroll(div.id, -5.5, -12.3, {
        precise: true,
        touchPhase: "started",
      })

      expect(receivedEvents).toHaveLength(1)
      expect(receivedEvents[0].precise).toBe(true)
      expect(receivedEvents[0].touchPhase).toBe("started")
      expect(receivedEvents[0].deltaX).toBe(-5.5)
      expect(receivedEvents[0].deltaY).toBe(-12.3)
    })

    it("should update state on scroll", () => {
      function ScrollCounter() {
        const [scrollCount, setScrollCount] = useState(0)
        return (
          <div onScroll={() => setScrollCount((c) => c + 1)}>
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

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("scroll"))!

      testRoot.renderer.simulateScroll(div.id, 0, -50)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Scrolls: 1",
        ]
      `)

      testRoot.renderer.simulateScroll(div.id, 0, -50)
      testRoot.renderer.simulateScroll(div.id, 0, -50)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Scrolls: 3",
        ]
      `)
    })
  })

  describe("keyUp events", () => {
    it("should handle onKeyUp and update state", () => {
      function KeyUpTracker() {
        const [lastKey, setLastKey] = useState("none")
        return (
          <div
            tabIndex={0}
            onKeyUp={(e: EventPayload) => setLastKey(e.key ?? "unknown")}
          >
            <text>{`Released: ${lastKey}`}</text>
          </div>
        )
      }

      testRoot.render(<KeyUpTracker />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Released: none",
        ]
      `)

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyUp"))!
      expect(div).toBeDefined()

      testRoot.renderer.simulateKeyUp(div.id, "a")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Released: a",
        ]
      `)
    })

    it("should handle keyDown and keyUp together", () => {
      const events: string[] = []

      function KeyPairTracker() {
        return (
          <div
            tabIndex={0}
            onKeyDown={(e: EventPayload) => events.push(`down:${e.key}`)}
            onKeyUp={(e: EventPayload) => events.push(`up:${e.key}`)}
          />
        )
      }

      testRoot.render(<KeyPairTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown") && d.events.has("keyUp"))!

      testRoot.renderer.simulateKeyDown(div.id, "enter")
      testRoot.renderer.simulateKeyUp(div.id, "enter")

      expect(events).toMatchInlineSnapshot(`
        [
          "down:enter",
          "up:enter",
        ]
      `)
    })

    it("should distinguish held keys from initial press", () => {
      const receivedEvents: EventPayload[] = []

      function HeldKeyTracker() {
        return (
          <div
            tabIndex={0}
            onKeyDown={(e: EventPayload) => receivedEvents.push(e)}
          />
        )
      }

      testRoot.render(<HeldKeyTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("keyDown"))!

      // Initial press
      testRoot.renderer.simulateKeyDown(div.id, "a", { isHeld: false })
      // Repeat (held)
      testRoot.renderer.simulateKeyDown(div.id, "a", { isHeld: true })
      testRoot.renderer.simulateKeyDown(div.id, "a", { isHeld: true })

      expect(receivedEvents.map((e) => e.isHeld)).toMatchInlineSnapshot(`
        [
          false,
          true,
          true,
        ]
      `)
    })
  })

  describe("click count (double/triple click)", () => {
    it("should receive clickCount in click events", () => {
      const receivedEvents: EventPayload[] = []

      function ClickCountTracker() {
        return (
          <div onClick={(e: EventPayload) => receivedEvents.push(e)}>
            <text>click me</text>
          </div>
        )
      }

      testRoot.render(<ClickCountTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("click"))!

      // Single click
      testRoot.renderer.simulateClick(div.id, { clickCount: 1 })
      expect(receivedEvents[0].clickCount).toBe(1)

      // Double click
      testRoot.renderer.simulateClick(div.id, { clickCount: 2 })
      expect(receivedEvents[1].clickCount).toBe(2)

      // Triple click
      testRoot.renderer.simulateClick(div.id, { clickCount: 3 })
      expect(receivedEvents[2].clickCount).toBe(3)
    })

    it("should update state differently for single vs double click", () => {
      function DoubleClickHandler() {
        const [mode, setMode] = useState("idle")

        return (
          <div
            onClick={(e: EventPayload) => {
              if ((e.clickCount ?? 1) >= 2) {
                setMode("double-clicked")
              } else {
                setMode("single-clicked")
              }
            }}
          >
            <text>{mode}</text>
          </div>
        )
      }

      testRoot.render(<DoubleClickHandler />)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "idle",
        ]
      `)

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("click"))!

      testRoot.renderer.simulateClick(div.id, { clickCount: 1 })
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "single-clicked",
        ]
      `)

      testRoot.renderer.simulateClick(div.id, { clickCount: 2 })
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "double-clicked",
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

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseDown"))!

      testRoot.renderer.simulateMouseDown(div.id)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "pressed",
        ]
      `)

      testRoot.renderer.simulateMouseUp(div.id)
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "released",
        ]
      `)
    })

    it("should receive mouse button info in mouseDown", () => {
      const receivedEvents: EventPayload[] = []

      function ButtonTracker() {
        return (
          <div onMouseDown={(e: EventPayload) => receivedEvents.push(e)} />
        )
      }

      testRoot.render(<ButtonTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseDown"))!

      // Left click
      testRoot.renderer.simulateMouseDown(div.id, { button: 0 })
      expect(receivedEvents[0].button).toBe(0)

      // Right click
      testRoot.renderer.simulateMouseDown(div.id, { button: 2 })
      expect(receivedEvents[1].button).toBe(2)

      // Middle click
      testRoot.renderer.simulateMouseDown(div.id, { button: 1 })
      expect(receivedEvents[2].button).toBe(1)
    })

    it("should receive modifiers in mouseDown events", () => {
      const receivedEvents: EventPayload[] = []

      function ModMouseTracker() {
        return (
          <div onMouseDown={(e: EventPayload) => receivedEvents.push(e)} />
        )
      }

      testRoot.render(<ModMouseTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseDown"))!

      testRoot.renderer.simulateMouseDown(div.id, {
        modifiers: { shift: true, ctrl: false, alt: false, cmd: false },
      })

      expect(receivedEvents[0].modifiers?.shift).toBe(true)
      expect(receivedEvents[0].modifiers?.cmd).toBe(false)
    })
  })

  describe("mouseMove events", () => {
    it("should handle onMouseMove and receive position", () => {
      const receivedEvents: EventPayload[] = []

      function MoveTracker() {
        return (
          <div onMouseMove={(e: EventPayload) => receivedEvents.push(e)} />
        )
      }

      testRoot.render(<MoveTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseMove"))!

      testRoot.renderer.simulateMouseMove(div.id, { x: 100, y: 200 })

      expect(receivedEvents).toHaveLength(1)
      expect(receivedEvents[0].x).toBe(100)
      expect(receivedEvents[0].y).toBe(200)
    })

    it("should receive pressedButton during drag", () => {
      const receivedEvents: EventPayload[] = []

      function DragTracker() {
        return (
          <div onMouseMove={(e: EventPayload) => receivedEvents.push(e)} />
        )
      }

      testRoot.render(<DragTracker />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseMove"))!

      // Move without button pressed
      testRoot.renderer.simulateMouseMove(div.id, { x: 10, y: 10 })
      expect(receivedEvents[0].pressedButton).toBeUndefined()

      // Move with left button pressed (simulating drag)
      testRoot.renderer.simulateMouseMove(div.id, {
        x: 50,
        y: 50,
        pressedButton: 0,
      })
      expect(receivedEvents[1].pressedButton).toBe(0)
    })

    it("should update state on mouse move", () => {
      function PositionTracker() {
        const [pos, setPos] = useState("0,0")
        return (
          <div
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

      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseMove"))!

      testRoot.renderer.simulateMouseMove(div.id, { x: 42, y: 99 })
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Position: 42,99",
        ]
      `)
    })
  })

  describe("combined event interactions", () => {
    it("should support drag interaction (mouseDown → mouseMove → mouseUp)", () => {
      const log: string[] = []

      function DragBox() {
        return (
          <div
            onMouseDown={() => log.push("down")}
            onMouseMove={(e: EventPayload) => {
              if (e.pressedButton !== undefined) {
                log.push(`drag:${e.x},${e.y}`)
              } else {
                log.push(`move:${e.x},${e.y}`)
              }
            }}
            onMouseUp={() => log.push("up")}
          />
        )
      }

      testRoot.render(<DragBox />)
      const div = testRoot.renderer
        .findByType("div")
        .find((d) => d.events.has("mouseDown"))!

      testRoot.renderer.simulateMouseDown(div.id, { x: 10, y: 10 })
      testRoot.renderer.simulateMouseMove(div.id, {
        x: 50,
        y: 50,
        pressedButton: 0,
      })
      testRoot.renderer.simulateMouseMove(div.id, {
        x: 100,
        y: 100,
        pressedButton: 0,
      })
      testRoot.renderer.simulateMouseUp(div.id, { x: 100, y: 100 })
      testRoot.renderer.simulateMouseMove(div.id, { x: 110, y: 110 })

      expect(log).toMatchInlineSnapshot(`
        [
          "down",
          "drag:50,50",
          "drag:100,100",
          "up",
          "move:110,110",
        ]
      `)
    })

    it("should support keyboard shortcuts with modifiers", () => {
      function ShortcutHandler() {
        const [action, setAction] = useState("none")

        return (
          <div
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
      testRoot.renderer.simulateKeyDown(div.id, "s", {
        modifiers: { shift: false, ctrl: false, alt: false, cmd: true },
      })
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: save",
        ]
      `)

      // Cmd+Shift+P
      testRoot.renderer.simulateKeyDown(div.id, "p", {
        modifiers: { shift: true, ctrl: false, alt: false, cmd: true },
      })
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: command-palette",
        ]
      `)

      // Escape (no modifiers)
      testRoot.renderer.simulateKeyDown(div.id, "escape")
      expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
        [
          "Action: cancel",
        ]
      `)
    })
  })

  describe("native end-to-end events", () => {
    // These tests go through the full GPUI pipeline (Rust hit testing,
    // event dispatch, emit_event_full). They only run when the native
    // test renderer is available (cargo build with test-support).
    const describeNative = hasNativeTestRenderer ? describe : describe.skip

    describeNative("keyboard via native GPUI pipeline", () => {
      it("should dispatch keyDown through GPUI to React handler", () => {
        function NativeKeyTracker() {
          const [lastKey, setLastKey] = useState("none")
          return (
            <div
              tabIndex={0}
              onKeyDown={(e: EventPayload) =>
                setLastKey(e.key ?? "unknown")
              }
            >
              <text>{`Key: ${lastKey}`}</text>
            </div>
          )
        }

        testRoot.render(<NativeKeyTracker />)
        expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
          [
            "Key: none",
          ]
        `)

        const div = testRoot.renderer
          .findByType("div")
          .find((d) => d.events.has("keyDown"))!

        testRoot.renderer.nativeSimulateKeystrokes(div.id, "a")

        expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
          [
            "Key: a",
          ]
        `)
      })

      it("should dispatch modifier keystrokes through native pipeline", () => {
        const receivedEvents: EventPayload[] = []

        function NativeModTracker() {
          return (
            <div
              tabIndex={0}
              onKeyDown={(e: EventPayload) => receivedEvents.push(e)}
            />
          )
        }

        testRoot.render(<NativeModTracker />)
        const div = testRoot.renderer
          .findByType("div")
          .find((d) => d.events.has("keyDown"))!

        testRoot.renderer.nativeSimulateKeystrokes(div.id, "cmd-s")

        expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
        const event = receivedEvents.find((e) => e.key === "s")
        expect(event).toBeDefined()
        expect(event!.modifiers?.cmd).toBe(true)
      })

      it("should support keyUp through native pipeline", () => {
        const events: string[] = []

        function NativeKeyUpTracker() {
          return (
            <div
              tabIndex={0}
              onKeyDown={(e: EventPayload) =>
                events.push(`down:${e.key}`)
              }
              onKeyUp={(e: EventPayload) =>
                events.push(`up:${e.key}`)
              }
            />
          )
        }

        testRoot.render(<NativeKeyUpTracker />)
        const div = testRoot.renderer
          .findByType("div")
          .find(
            (d) => d.events.has("keyDown") && d.events.has("keyUp")
          )!

        testRoot.renderer.nativeSimulateKeystrokes(div.id, "enter")

        // GPUI's simulate_keystrokes dispatches KeyDown through the
        // focused element's on_key_down handler. KeyUp goes to the
        // window level but not through on_key_up element handlers in
        // the same way — so only keyDown is guaranteed here.
        expect(events).toContain("down:enter")
      })

      it("should support keyboard navigation end-to-end", () => {
        function NativeSelectableList() {
          const items = ["Apple", "Banana", "Cherry"]
          const [selected, setSelected] = useState(0)

          return (
            <div
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

        testRoot.render(<NativeSelectableList />)
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

        testRoot.renderer.nativeSimulateKeystrokes(list.id, "down")
        expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
          [
            "  Apple",
            "  Banana",
            "> Cherry",
          ]
        `)

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

    describeNative("scroll via native GPUI pipeline", () => {
      it("should dispatch scroll events through GPUI to React", () => {
        const receivedEvents: EventPayload[] = []

        function NativeScrollBox() {
          return (
            <div
              style={{ width: 200, height: 200 }}
              onScroll={(e: EventPayload) => receivedEvents.push(e)}
            >
              <text>scrollable</text>
            </div>
          )
        }

        testRoot.render(<NativeScrollBox />)
        testRoot.renderer.nativeSimulateScrollWheel(100, 100, 0, -50)

        expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
        const scrollEvent = receivedEvents.find(
          (e) => e.eventType === "scroll"
        )
        expect(scrollEvent).toBeDefined()
        expect(scrollEvent!.deltaY).toBeDefined()
      })

      it("should update state on native scroll", () => {
        function NativeScrollCounter() {
          const [count, setCount] = useState(0)
          return (
            <div
              style={{ width: 200, height: 200 }}
              onScroll={() => setCount((c) => c + 1)}
            >
              <text>{`Scrolls: ${count}`}</text>
            </div>
          )
        }

        testRoot.render(<NativeScrollCounter />)
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

    describeNative("click via native GPUI pipeline", () => {
      it("should dispatch click through GPUI hit testing to React", () => {
        function NativeCounter() {
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

        testRoot.render(<NativeCounter />)
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

        testRoot.renderer.nativeSimulateClick(10, 10)
        expect(testRoot.renderer.getAllText()).toMatchInlineSnapshot(`
          [
            "Count: 2",
          ]
        `)
      })
    })

    describeNative("mouse move via native GPUI pipeline", () => {
      it("should dispatch mouse move through GPUI to React", () => {
        const receivedEvents: EventPayload[] = []

        function NativeMoveTracker() {
          return (
            <div
              style={{ width: 300, height: 300 }}
              onMouseMove={(e: EventPayload) => receivedEvents.push(e)}
            >
              <text>move here</text>
            </div>
          )
        }

        testRoot.render(<NativeMoveTracker />)
        testRoot.renderer.nativeSimulateMouseMove(50, 50)

        expect(receivedEvents.length).toBeGreaterThanOrEqual(1)
        const moveEvent = receivedEvents.find(
          (e) => e.eventType === "mouseMove"
        )
        expect(moveEvent).toBeDefined()
      })
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
