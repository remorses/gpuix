/**
 * Scrollbars on scroll boxes. `GPUIX_SCROLLBARS` picks the kind of bar,
 * so the tests do not depend on the machine's setting.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

type Style = React.CSSProperties & Record<string, string | number | undefined>

function Page({ box, content = 400 }: { box: Style; content?: number }) {
  return (
    <div style={{ width: 200, height: 100, ...box }}>
      <div id="inner" style={{ width: "100%", height: content }}>
        <text>inner</text>
      </div>
    </div>
  )
}

describeNative("scrollbars", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot({})
  })
  afterEach(() => {
    root.unmount()
    delete process.env.GPUIX_SCROLLBARS
  })

  const box = () => root.renderer.findByType("div")[0]!
  const inner = () => root.renderer.findByType("div")[1]!
  /** The inner box's rectangle after a settled frame. */
  const innerBounds = () => {
    root.renderer.getElementBounds(inner().id)
    return root.renderer.getElementBounds(inner().id)!
  }

  describe("classic bars", () => {
    beforeEach(() => {
      process.env.GPUIX_SCROLLBARS = "classic"
    })

    it("reserve a 15px gutter for overflow: scroll", () => {
      root.render(<Page box={{ overflow: "scroll" }} content={50} />)
      expect(innerBounds()[2]).toBe(185)
    })

    it("reserve the gutter for overflow: auto only when the content overflows", () => {
      root.render(<Page box={{ overflowY: "auto" }} content={50} />)
      expect(innerBounds()[2]).toBe(200)
      root.render(<Page box={{ overflowY: "auto" }} content={400} />)
      expect(innerBounds()[2]).toBe(185)
    })

    it("follow scrollbar-width and scrollbar-gutter", () => {
      root.render(<Page box={{ overflowY: "scroll", scrollbarWidth: "thin" }} />)
      expect(innerBounds()[2]).toBe(192)
      root.render(<Page box={{ overflowY: "scroll", scrollbarWidth: "none" }} />)
      expect(innerBounds()[2]).toBe(200)
      root.render(<Page box={{ overflowY: "auto", scrollbarGutter: "stable" }} content={50} />)
      expect(innerBounds()[2]).toBe(185)
      root.render(
        <Page box={{ overflowY: "auto", scrollbarGutter: "stable both-edges" }} content={50} />
      )
      const [x, , width] = innerBounds()
      expect(x).toBe(15)
      expect(width).toBe(170)
    })

    it("scroll with a thumb drag and a track click", () => {
      root.render(<Page box={{ overflowY: "scroll" }} />)
      const id = box().id
      // The thumb is a quarter of the track, at the top. Take it 40px down.
      root.renderer.nativeSimulateMouseDown(192, 10)
      root.renderer.nativeSimulateMouseMove(192, 50, 0)
      root.renderer.nativeSimulateMouseUp(192, 50)
      const dragged = root.renderer.getScrollOffset(id)![1]
      expect(dragged).toBeCloseTo(-160, 0)

      // A click in the track below the thumb pages down.
      root.renderer.nativeSimulateMouseDown(192, 98)
      root.renderer.nativeSimulateMouseUp(192, 98)
      expect(root.renderer.getScrollOffset(id)![1]).toBeCloseTo(-250, 0)
    })
  })

  describe("overlay bars", () => {
    beforeEach(() => {
      process.env.GPUIX_SCROLLBARS = "overlay"
    })

    it("reserve nothing, even with scrollbar-gutter: stable", () => {
      root.render(<Page box={{ overflow: "scroll", scrollbarGutter: "stable" }} />)
      expect(innerBounds()[2]).toBe(200)
    })

    it("take a thumb drag after a scroll", () => {
      root.render(<Page box={{ overflowY: "scroll" }} />)
      const id = box().id
      root.renderer.nativeSimulateScrollWheel(100, 50, 0, -10)
      const before = root.renderer.getScrollOffset(id)![1]
      expect(before).toBeLessThan(0)
      root.renderer.nativeSimulateMouseDown(195, 12)
      root.renderer.nativeSimulateMouseMove(195, 60, 0)
      root.renderer.nativeSimulateMouseUp(195, 60)
      expect(root.renderer.getScrollOffset(id)![1]).toBeLessThan(before - 100)
    })
  })

  it("overflow: auto scrolls like overflow: scroll", () => {
    root.render(<Page box={{ overflowY: "auto" }} />)
    root.renderer.nativeSimulateScrollWheel(100, 50, 0, -30)
    expect(root.renderer.getScrollOffset(box().id)![1]).toBeLessThan(0)
  })
})

describeNative("scrollIntoView", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot({})
  })
  afterEach(() => root.unmount())

  function List({ margin, padding }: { margin?: number; padding?: number }) {
    return (
      <div style={{ width: 200, height: 100, overflowY: "auto", scrollPaddingTop: padding }}>
        {Array.from({ length: 10 }, (_, i) => (
          <div key={i} style={{ height: 40, scrollMarginTop: i === 6 ? margin : undefined }}>
            <text>{`row ${i}`}</text>
          </div>
        ))}
      </div>
    )
  }

  it("brings the target to the start and honours the margins", () => {
    root.render(<List margin={4} padding={10} />)
    const divs = root.renderer.findByType("div")
    root.renderer.scrollIntoView(divs[7]!.id)
    // The target's top is at 240. scroll-padding keeps 10 inside the
    // box and scroll-margin keeps 4 around the target: 240 - 10 - 4.
    expect(root.renderer.getScrollOffset(divs[0]!.id)![1]).toBe(-226)
  })

  it("a repeated call moves nothing", () => {
    // The box's own recorded bounds must not move with its own offset.
    // When they do, the second call applies the same delta again.
    root.render(<List />)
    const divs = root.renderer.findByType("div")
    root.renderer.scrollIntoView(divs[7]!.id)
    const once = root.renderer.getScrollOffset(divs[0]!.id)![1]
    expect(once).toBeLessThan(0)
    root.renderer.scrollIntoView(divs[7]!.id)
    expect(root.renderer.getScrollOffset(divs[0]!.id)![1]).toBe(once)
  })

  it("nearest leaves a visible target alone", () => {
    root.render(<List />)
    const divs = root.renderer.findByType("div")
    root.renderer.scrollIntoView(divs[2]!.id, "nearest")
    expect(root.renderer.getScrollOffset(divs[0]!.id)![1]).toBe(0)
    root.renderer.scrollIntoView(divs[7]!.id, "end")
    // The target's bottom is at 280 and the viewport is 100 tall.
    expect(root.renderer.getScrollOffset(divs[0]!.id)![1]).toBe(-180)
  })
})
