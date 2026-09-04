/**
 * scroll-behavior, scroll-snap and scroll-initial-target. The automation
 * clock is paused, so the glide of an offset is read at exact times.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"
import type { StyleDesc } from "../types/host"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/** A scroll box of 200 by 200 with `count` rows of 100. */
function Box({ style, rows }: { style?: StyleDesc; rows: StyleDesc[] }) {
  return (
    <div testId="box" style={{ width: 200, height: 200, overflowY: "auto", ...style }}>
      {rows.map((row, i) => (
        <div key={i} testId={`row-${i}`} style={{ width: 200, height: 100, flexShrink: 0, ...row }} />
      ))}
    </div>
  )
}

const plain = (count: number): StyleDesc[] => Array.from({ length: count }, () => ({}))

describeNative("scroll-behavior", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  const boxId = () => root.renderer.findByTestId("box")!.id
  const offsetY = (id: number) => root.renderer.getScrollOffset(id)![1]

  it("scrollTo glides when the box asks for smooth", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -200)
    expect(offsetY(id)).toBeCloseTo(0, 5)

    root.renderer.clockFastForward(150)
    root.renderer.flush()
    const midway = offsetY(id)
    expect(midway).toBeLessThan(0)
    expect(midway).toBeGreaterThan(-200)

    root.renderer.clockFastForward(400)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-200)
  })

  it("an instant behavior beats the style, and smooth beats auto", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -200, "instant")
    expect(offsetY(id)).toBe(-200)

    root.render(<Box rows={plain(6)} />)
    root.renderer.scrollTo(id, 0, 0, "smooth")
    expect(offsetY(id)).toBe(-200)
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(0)
  })

  it("a direct offset move cancels the glide", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -300)
    root.renderer.clockFastForward(100)
    root.renderer.flush()

    // The user takes over. The glide must not fight the new offset.
    root.renderer.scrollTo(id, 0, -50, "instant")
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-50)
  })

  it("scrollIntoView glides too", () => {
    root.render(<Box style={{ scrollBehavior: "smooth" }} rows={plain(6)} />)
    const id = boxId()
    const target = root.renderer.findByTestId("row-3")!
    root.renderer.scrollIntoView(target.id, "start")
    expect(offsetY(id)).toBeCloseTo(0, 5)
    root.renderer.clockFastForward(500)
    root.renderer.flush()
    expect(offsetY(id)).toBe(-300)
  })
})

describeNative("scroll snap", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  const boxId = () => root.renderer.findByTestId("box")!.id
  const offsetY = (id: number) => root.renderer.getScrollOffset(id)![1]

  /** Rest, then let the snap glide finish: one frame past the idle time
   *  arms the glide, and one more period lands it. */
  const settle = () => {
    root.renderer.clockFastForward(200)
    root.renderer.flush()
    root.renderer.clockFastForward(400)
    root.renderer.flush()
    root.renderer.flush()
  }

  it("a mandatory container rests on the nearest snap position", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(<Box style={{ scrollSnapType: "y mandatory" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -130, "instant")
    settle()
    expect(offsetY(id)).toBe(-100)
  })

  /** A trackpad gesture: fingers down, pulls of `delta` 8ms apart, lift. */
  const fling = (id: number, pulls: number, delta: number) => {
    const [x, y] = root.renderer.getElementBounds(id)!
    const at = { x: x + 100, y: y + 100 }
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, 0, undefined, "started")
    for (let i = 0; i < pulls; i++) {
      root.renderer.clockFastForward(8)
      root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, delta, undefined, "moved")
    }
    root.renderer.clockFastForward(8)
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, 0, undefined, "ended")
    return at
  }

  /** Let the glide the lift started take its start time and land. The
   *  fling curve runs about 0.6s over these distances. */
  const glide = () => {
    root.renderer.flush()
    root.renderer.clockFastForward(700)
    root.renderer.flush()
    root.renderer.flush()
  }

  it("a fling snaps at the lift, to the predicted landing", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    // Four pulls of 40px in 32ms is 5000px/s: the momentum would run
    // far past the end, so the glide goes straight to the last row.
    fling(id, 4, -40)
    glide()
    expect(offsetY(id)).toBe(-400)
  })

  it("the momentum after the lift cannot cancel the glide", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    // A gentler fling: 20px in 32ms lands near -330, so row 3 at -300.
    const at = fling(id, 4, -5)
    root.renderer.flush()
    // The OS momentum stream after the lift. Consumed, so it cannot
    // move the box off the glide.
    for (let i = 0; i < 3; i++) {
      root.renderer.clockFastForward(16)
      root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, -30, undefined, "moved")
    }
    glide()
    expect(offsetY(id)).toBe(-300)
  })

  it("the box does not snap while the fingers stay on the pad", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    const [x, y] = root.renderer.getElementBounds(id)!
    const at = { x: x + 100, y: y + 100 }
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, 0, undefined, "started")
    for (let i = 0; i < 4; i++) {
      root.renderer.clockFastForward(8)
      root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, -5, undefined, "moved")
    }
    root.renderer.flush()
    // The fingers rest on the pad, far past the idle window. The web
    // never snaps during the drag, so the box must hold at -20. The
    // second wait gives a wrongly started glide the time to land.
    root.renderer.clockFastForward(300)
    root.renderer.flush()
    root.renderer.clockFastForward(400)
    root.renderer.flush()
    root.renderer.flush()
    expect(offsetY(id)).toBe(-20)
    // The lift after the rest has zero velocity, so the box snaps to
    // the nearest position, row 1 at 0.
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, 0, undefined, "ended")
    glide()
    expect(offsetY(id)).toBe(0)
  })

  it("the momentum tail after the glide lands cannot move the box", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    const at = fling(id, 4, -5)
    root.renderer.flush()
    // The glide runs about 0.62s, and the OS stream keeps sending an
    // event every 16ms well past the landing. Chromium consumes the
    // whole stream until the next gesture begin, so the tail cannot
    // push the box off the snap position.
    for (let i = 0; i < 50; i++) {
      root.renderer.clockFastForward(16)
      root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, -30, undefined, "moved")
      root.renderer.flush()
    }
    expect(offsetY(id)).toBe(-300)
    // A gap of more than 100ms ends the stream, so the next phaseless
    // wheel is a new scroll and moves the box.
    root.renderer.clockFastForward(200)
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, -30, undefined, "moved")
    root.renderer.flush()
    expect(offsetY(id)).toBe(-330)
  })

  it("the fling glide decays like momentum, not like a fixed ease", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    fling(id, 4, -5)
    root.renderer.flush()
    // Chromium's curve takes 39 frames of 16ms for the 280px that are
    // left, so at 320ms the box still moves. The old fixed 300ms ease
    // had already landed.
    root.renderer.clockFastForward(320)
    root.renderer.flush()
    const mid = offsetY(id)
    expect(mid).toBeLessThan(-200)
    expect(mid).toBeGreaterThan(-295)
    root.renderer.clockFastForward(400)
    root.renderer.flush()
    root.renderer.flush()
    expect(offsetY(id)).toBe(-300)
  })

  it("the sub-pixel tail of a scroll does not delay the snap", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(<Box style={{ scrollSnapType: "y mandatory" }} rows={rows} />)
    const id = boxId()
    // A real move, then a tail of steps under half a pixel, the way a
    // wheel coasts. The tail must not reset the idle timer.
    root.renderer.scrollTo(id, 0, -130, "instant")
    for (let i = 1; i <= 4; i++) {
      root.renderer.clockFastForward(16)
      root.renderer.scrollTo(id, 0, -130 - 0.3 * i, "instant")
    }
    // 124ms after the real move the glide is armed. 320ms more lands
    // it. The old 150ms window, reset by the tail, would still glide.
    root.renderer.clockFastForward(60)
    root.renderer.flush()
    root.renderer.flush()
    root.renderer.clockFastForward(320)
    root.renderer.flush()
    root.renderer.flush()
    expect(offsetY(id)).toBe(-100)
  })

  it("proximity gives up beyond half a viewport", () => {
    const rows = plain(8).map((_, i) =>
      i === 0 || i === 7 ? { scrollSnapAlign: "start" } : {}
    )
    root.render(<Box style={{ scrollSnapType: "y proximity" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -280, "instant")
    settle()
    expect(offsetY(id)).toBe(-280)
  })

  it("scroll-snap-stop always catches a long scroll", () => {
    const rows = plain(8).map((_, i) => ({
      scrollSnapAlign: "start",
      scrollSnapStop: i === 2 ? "always" : undefined,
    }))
    root.render(<Box style={{ scrollSnapType: "y mandatory" }} rows={rows} />)
    const id = boxId()
    root.renderer.scrollTo(id, 0, -440, "instant")
    settle()
    expect(offsetY(id)).toBe(-200)
  })

  it("a fling stops at an always area it passes", () => {
    const rows = plain(10).map((_, i) => ({
      scrollSnapAlign: "start",
      scrollSnapStop: i === 1 ? "always" : undefined,
    }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    // The lift at -40 predicts a landing near -540, past row 1 at -100.
    // The always area stops the glide there.
    fling(id, 4, -10)
    glide()
    expect(offsetY(id)).toBe(-100)
  })

  it("the lift asks the window for the paint that starts the glide", () => {
    const rows = plain(6).map(() => ({ scrollSnapAlign: "start" }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    const [x, y] = root.renderer.getElementBounds(id)!
    const at = { x: x + 100, y: y + 100 }
    root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, 0, undefined, "started")
    for (let i = 0; i < 4; i++) {
      root.renderer.clockFastForward(8)
      root.renderer.nativeSimulateScrollWheel(at.x, at.y, 0, -40, undefined, "moved")
    }
    // A flush paints and clears the redraw mark, so the probes below
    // read only what their own dispatch asked for.
    root.renderer.flush()
    root.renderer.clockFastForward(8)
    // The glide moves one step per painted frame, and the live window
    // only paints when something asks. The lift must ask: with no mark
    // here, the glide sat inert and the fling stopped dead.
    expect(
      root.renderer.nativeSimulateScrollWheelProbe(at.x, at.y, 0, 0, undefined, "ended")
    ).toBe(true)
    // The consumed momentum events schedule no paint by themselves, so
    // each one must also ask, in case a paint cancelled the glide and
    // the chain of requested frames ended.
    root.renderer.flush()
    root.renderer.clockFastForward(16)
    expect(
      root.renderer.nativeSimulateScrollWheelProbe(at.x, at.y, 0, -30, undefined, "moved")
    ).toBe(true)
    // The glide still lands: the probe dispatches real events.
    glide()
    expect(offsetY(id)).toBe(-400)
  })

  it("an always area the drag passed cannot pull the fling back", () => {
    const rows = plain(10).map((_, i) => ({
      scrollSnapAlign: "start",
      scrollSnapStop: i === 1 ? "always" : undefined,
    }))
    root.render(
      <Box style={{ scrollSnapType: "y mandatory", scrollbarWidth: "none" }} rows={rows} />
    )
    const id = boxId()
    // The drag takes the box to -150, past the always row at -100. The
    // fling from the lift passes only positions below -150, so the row
    // the fingers already passed must not pull the box back up.
    fling(id, 5, -30)
    glide()
    // The 650px glide takes 49 fling frames, more than one `glide`.
    root.renderer.clockFastForward(300)
    root.renderer.flush()
    root.renderer.flush()
    expect(offsetY(id)).toBe(-800)
  })
})

describeNative("the container option of scrollIntoView", () => {
  /** An outer scroll box that holds a filler row and an inner one. */
  function Nested() {
    return (
      <div testId="outer" style={{ width: 200, height: 200, overflowY: "auto" }}>
        <div style={{ width: 200, height: 300, flexShrink: 0 }} />
        <div
          testId="inner"
          style={{ width: 200, height: 200, overflowY: "auto", flexShrink: 0 }}
        >
          {plain(6).map((_, i) => (
            <div
              key={i}
              testId={`inner-row-${i}`}
              style={{ width: 200, height: 100, flexShrink: 0 }}
            />
          ))}
        </div>
      </div>
    )
  }

  it("nearest scrolls only the nearest scroll box", () => {
    const root = createTestRoot()
    root.render(<Nested />)
    const outer = root.renderer.findByTestId("outer")!
    const inner = root.renderer.findByTestId("inner")!
    const target = root.renderer.findByTestId("inner-row-3")!
    root.renderer.scrollIntoView(target.id, "start", undefined, undefined, "nearest")
    expect(root.renderer.getScrollOffset(inner.id)![1]).toBe(-300)
    expect(root.renderer.getScrollOffset(outer.id)![1]).toBe(0)
    root.unmount()
  })

  it("all, the default, scrolls every ancestor", () => {
    const root = createTestRoot()
    root.render(<Nested />)
    const outer = root.renderer.findByTestId("outer")!
    const inner = root.renderer.findByTestId("inner")!
    const target = root.renderer.findByTestId("inner-row-3")!
    root.renderer.scrollIntoView(target.id, "start")
    expect(root.renderer.getScrollOffset(inner.id)![1]).toBe(-300)
    expect(root.renderer.getScrollOffset(outer.id)![1]).toBe(-300)
    root.unmount()
  })
})

describeNative("logical scroll margins and paddings", () => {
  it("scrollIntoView reads the block variants", () => {
    const root = createTestRoot()
    const rows = plain(8).map((_, i) =>
      i === 5 ? { scrollMarginBlockStart: 16 } : {}
    )
    root.render(<Box style={{ scrollPaddingBlock: 12 }} rows={rows} />)
    const box = root.renderer.findByTestId("box")!
    const target = root.renderer.findByTestId("row-5")!
    root.renderer.scrollIntoView(target.id, "start")
    // The row sits at 500. The box keeps 12 inside its edge and the row
    // asks for 16 above itself, so the offset is 500 - 28.
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-472)
    root.unmount()
  })
})

describeNative("scroll-initial-target", () => {
  it("scrolls the box to the element when it first paints", () => {
    const root = createTestRoot()
    root.renderer.clockPause()
    const rows = plain(8).map((_, i) =>
      i === 5 ? { scrollInitialTarget: "nearest" } : {}
    )
    root.render(<Box rows={rows} />)
    const box = root.renderer.findByTestId("box")!
    // The first frame paints the bounds, the second one reads them.
    root.renderer.flush()
    root.renderer.flush()
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-500)
    root.unmount()
  })
})

describeNative("scroll restore", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
    root.renderer.clockPause()
  })
  afterEach(() => {
    root.unmount()
  })

  it("a scrollTo from the mount effect applies on the first frame", () => {
    // A mount effect runs in the commit that creates the element, so the
    // call comes before the element ever painted. The offset must stick,
    // and the first painted frame must show it.
    function Restored() {
      const box = React.useRef<{ id: number } | null>(null)
      React.useLayoutEffect(() => {
        root.renderer.scrollTo(box.current!.id, 0, -150, "instant")
      }, [])
      return (
        <div ref={box} testId="box" style={{ width: 200, height: 200, overflowY: "auto" }}>
          {plain(6).map((_, i) => (
            <div key={i} testId={`row-${i}`} style={{ width: 200, height: 100, flexShrink: 0 }} />
          ))}
        </div>
      )
    }
    root.render(<Restored />)
    const box = root.renderer.findByTestId("box")!
    expect(root.renderer.getScrollOffset(box.id)![1]).toBe(-150)
    // The first painted frame already has the offset.
    expect(root.renderer.getElementBounds(root.renderer.findByTestId("row-0")!.id)![1]).toBe(-150)
  })

  it("an app saves and restores the offset across a remount", () => {
    // The pattern of the demo navigation: the cleanup saves the offset
    // when a screen unmounts, and the mount effect sets it back when the
    // user goes back, the way iOS keeps the scroll position of a screen.
    const offsets = new Map<string, [number, number]>()
    function Screen({ id }: { id: string }) {
      const box = React.useRef<{ id: number } | null>(null)
      React.useLayoutEffect(() => {
        const el = box.current!
        const saved = offsets.get(id)
        if (saved) root.renderer.scrollTo(el.id, saved[0], saved[1], "instant")
        return () => {
          const offset = root.renderer.getScrollOffset(el.id)
          if (offset) offsets.set(id, [offset[0]!, offset[1]!])
        }
      }, [])
      return (
        <div ref={box} testId={`screen-${id}`} style={{ width: 200, height: 200, overflowY: "auto" }}>
          {plain(6).map((_, i) => (
            <div key={i} style={{ width: 200, height: 100, flexShrink: 0 }} />
          ))}
        </div>
      )
    }
    root.render(<Screen key="a" id="a" />)
    const first = root.renderer.findByTestId("screen-a")!
    root.renderer.scrollTo(first.id, 0, -150, "instant")

    root.render(<Screen key="b" id="b" />)
    root.render(<Screen key="a" id="a" />)
    const again = root.renderer.findByTestId("screen-a")!
    expect(again.id).not.toBe(first.id)
    expect(root.renderer.getScrollOffset(again.id)![1]).toBe(-150)
  })
})
