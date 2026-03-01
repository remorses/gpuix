/// Tests for GPUIX event handling — verifies that React components
/// receive events and re-render correctly via the TestRenderer.
///
/// @ts-nocheck — JSX types resolve to DOM's HTMLDivElement, not GPUIX's Props.
/// The tests run correctly via vitest; tsc checking is skipped for test files
/// until we set up a custom JSX namespace for GPUIX elements.
// @ts-nocheck

import { describe, it, expect, beforeEach } from "vitest"
import React, { useState } from "react"
import { createTestRoot } from "../testing"
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
