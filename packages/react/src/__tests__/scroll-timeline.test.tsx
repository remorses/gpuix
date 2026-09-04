/**
 * scroll-timeline-*, animation-timeline and scroll-marker-group. The
 * automation clock is paused and then run far ahead, so a motion the
 * clock drives would sit at 100%. The scroll offset driving it instead
 * is what each assertion shows.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/** A named width motion from 0 to 100, for one timeline value. */
function Progress({ testId, timeline }: { testId: string; timeline: string }) {
  return (
    <div
      testId={testId}
      style={{ height: 10, animationTimeline: timeline }}
      motion={{
        initial: { width: 0 },
        animate: { width: 100 },
        transition: { duration: 0.1, ease: "linear" },
      }}
    />
  )
}

/** A scroll box of 200 by 200 with six rows of 100, so the box scrolls
 *  400px at most. */
function Box({ style, rows, children }: {
  style?: Record<string, unknown>
  rows?: Record<string, unknown>
  children?: React.ReactNode
}) {
  return (
    <div testId="box" style={{ width: 200, height: 200, overflowY: "auto", ...style }}>
      {children}
      {Array.from({ length: 6 }, (_, i) => (
        <div key={i} testId={`row-${i}`} style={{ width: 200, height: 100, flexShrink: 0, ...rows }} />
      ))}
    </div>
  )
}

describeNative("scroll timelines", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  const widthOf = (testId: string) => {
    const node = root.renderer.findByTestId(testId)!
    return root.renderer.getElementBounds(node.id)![2]!
  }

  it("a named timeline swaps the clock for the scroll offset", () => {
    root.render(
      <div style={{ width: 200 }}>
        <Progress testId="progress" timeline="--p" />
        <Progress testId="orphan" timeline="--missing" />
        <Box style={{ scrollTimelineName: "--p" }} />
      </div>
    )
    const box = root.renderer.findByTestId("box")!
    // Far past the 0.1s duration. A clock-driven motion would read 100.
    root.renderer.clockFastForward(2000)
    root.renderer.flush()
    expect(widthOf("progress")).toBe(0)

    // Half of the 400px range is progress 0.5.
    root.renderer.scrollTo(box.id, 0, -200, "instant")
    root.renderer.flush()
    expect(widthOf("progress")).toBeCloseTo(50, 1)

    root.renderer.scrollTo(box.id, 0, -400, "instant")
    root.renderer.flush()
    expect(widthOf("progress")).toBeCloseTo(100, 1)

    // A name no box declares holds the animation at 0.
    expect(widthOf("orphan")).toBe(0)
  })

  it("scroll(nearest) reads the nearest ancestor scroll box", () => {
    root.render(
      <Box>
        <Progress testId="inside" timeline="scroll(nearest)" />
      </Box>
    )
    const box = root.renderer.findByTestId("box")!
    root.renderer.clockFastForward(2000)
    root.renderer.flush()
    expect(widthOf("inside")).toBe(0)

    // The inner rows now total 610, so the range is 410.
    root.renderer.scrollTo(box.id, 0, -410, "instant")
    root.renderer.flush()
    expect(widthOf("inside")).toBeCloseTo(100, 1)
  })
})

describeNative("scroll-marker-group", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  it("a click on a marker scrolls to its snap area", () => {
    root.render(
      <Box
        style={{
          scrollSnapType: "y mandatory",
          scrollMarkerGroup: "before",
          scrollbarWidth: "none",
        }}
        rows={{ scrollSnapAlign: "start" }}
      />
    )
    const box = root.renderer.findByTestId("box")!
    // The first frame paints the bounds. The second one reads them and
    // paints the markers.
    root.renderer.flush()
    root.renderer.flush()
    const bounds = root.renderer.getElementBounds(box.id)!
    // Six areas make six 6px dots with 6px gaps, 66px in all, centred on
    // the left edge with a 16px inset. The fourth dot's centre:
    const x = bounds[0]! + 16 + 3
    const y = bounds[1]! + (200 - 66) / 2 + 3 * 12 + 3
    root.renderer.nativeSimulateClick(x, y)
    root.renderer.flush()
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-300)
  })
})
