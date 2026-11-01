import { describe, expect, it } from "vitest"
import type { EventPayload } from "@gpuix/native"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const size = (event: EventPayload) => [event.width, event.height]

describeNative("onLayout", () => {
  it("reports the border box after the first paint", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const events: EventPayload[] = []
      render(
        <div style={{ padding: 10 }}>
          <div
            testId="box"
            style={{ width: 120, height: 40, borderWidth: 2, borderColor: "#fff" }}
            onLayout={(event) => events.push(event)}
          />
        </div>
      )
      renderer.dispatchNativeEvents()

      expect(events).toHaveLength(1)
      expect(size(events[0]!)).toEqual([120, 40])
      const bounds = renderer.getElementBounds(renderer.findByTestId("box")!.id)!
      expect([events[0]!.x, events[0]!.y]).toEqual([bounds.x, bounds.y])
    } finally {
      unmount()
    }
  })

  it("reports again only when the size changes", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const sizes: unknown[] = []
      const app = (width: number, label = "a") => (
        <div style={{ flexDirection: "column" }}>
          <div
            style={{ width, height: 30 }}
            onLayout={(event) => sizes.push(size(event))}
            testId={label}
          />
        </div>
      )

      render(app(100))
      renderer.dispatchNativeEvents()
      render(app(100))
      renderer.dispatchNativeEvents()
      expect(sizes, "an identical frame reports nothing").toEqual([[100, 30]])

      render(app(140))
      renderer.dispatchNativeEvents()
      expect(sizes).toEqual([
        [100, 30],
        [140, 30],
      ])
    } finally {
      unmount()
    }
  })

  it("stays quiet when an element moves without resizing", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const sizes: unknown[] = []
      const app = (above: number) => (
        <div style={{ flexDirection: "column" }}>
          <div style={{ height: above, width: 50 }} />
          <div style={{ width: 80, height: 20 }} onLayout={(event) => sizes.push(size(event))} />
        </div>
      )

      render(app(10))
      renderer.dispatchNativeEvents()
      render(app(60))
      renderer.dispatchNativeEvents()
      expect(sizes).toEqual([[80, 20]])
    } finally {
      unmount()
    }
  })

  it("reports a size the element gets from its content", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const heights: number[] = []
      const app = (lines: number) => (
        <div
          style={{ width: 200, flexDirection: "column" }}
          onLayout={(event) => heights.push(event.height ?? -1)}
        >
          {Array.from({ length: lines }, (_, index) => (
            <div key={index} style={{ height: 16 }} />
          ))}
        </div>
      )

      render(app(1))
      renderer.dispatchNativeEvents()
      render(app(3))
      renderer.dispatchNativeEvents()
      expect(heights).toEqual([16, 48])
    } finally {
      unmount()
    }
  })

  it("reports again when the listener is removed and added back", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const sizes: unknown[] = []
      const app = (listening: boolean) => (
        <div
          style={{ width: 70, height: 70 }}
          onLayout={listening ? (event) => sizes.push(size(event)) : undefined}
        />
      )

      render(app(true))
      renderer.dispatchNativeEvents()
      render(app(false))
      renderer.dispatchNativeEvents()
      render(app(true))
      renderer.dispatchNativeEvents()
      expect(sizes).toEqual([
        [70, 70],
        [70, 70],
      ])
    } finally {
      unmount()
    }
  })

  it("is silent while the element is hidden, and reports once it shows", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const sizes: unknown[] = []
      const app = (visibility: "hidden" | "visible") => (
        <div
          style={{ width: 64, height: 24, visibility }}
          onLayout={(event) => sizes.push(size(event))}
        />
      )

      render(app("hidden"))
      renderer.dispatchNativeEvents()
      expect(sizes, "gpui paints nothing for a hidden element").toEqual([])

      render(app("visible"))
      renderer.dispatchNativeEvents()
      expect(sizes).toEqual([[64, 24]])
    } finally {
      unmount()
    }
  })

  it("reports elements inside a virtual list as they are measured", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      const sizes: unknown[][] = []
      render(
        <div style={{ width: 300, height: 100 }}>
          <virtual-list style={{ width: 300, height: 100 }} estimatedItemHeight={20}>
            {Array.from({ length: 50 }, (_, index) => (
              <div
                key={index}
                style={{ height: 20 + (index % 2) * 10 }}
                onLayout={(event) => sizes.push([index, event.height])}
              />
            ))}
          </virtual-list>
        </div>
      )
      renderer.dispatchNativeEvents()

      expect(sizes.length).toBeGreaterThan(0)
      expect(sizes.length, "rows outside the window are never built").toBeLessThan(50)
      for (const [index, height] of sizes) {
        expect(height).toBe(20 + ((index as number) % 2) * 10)
      }
    } finally {
      unmount()
    }
  })
})

describeNative("published frame", () => {
  it("publishes bounds, including deferred <anchored> layers, after paint", () => {
    const { render, renderer, unmount } = createTestRoot()
    try {
      render(
        <div>
          <div testId="plain" style={{ width: 50, height: 30 }} />
          <anchored testId="layer" position={{ x: 100, y: 100 }} style={{ width: 40, height: 20 }} />
        </div>
      )
      for (const testId of ["plain", "layer"]) {
        const id = renderer.findByTestId(testId)!.id
        const painted = renderer.getElementBounds(id)
        expect(painted, testId).not.toBeNull()
        expect(renderer.getPublishedElementBounds(id), testId).toEqual(painted)
      }
    } finally {
      unmount()
    }
  })
})
