/// The native <virtual-list>: lazy rows, programmatic scrolling, and chat tail following.

import React from "react"
import { afterEach, describe, expect, it } from "vitest"
import { createTestRoot } from "../testing.js"

function Rows({ count }: { count: number }) {
  return Array.from({ length: count }, (_, index) => (
    <div
      key={index}
      style={{
        display: "flex",
        height: 40,
        flexShrink: 0,
        alignItems: "center",
      }}
    >
      <text>{`row-${index}`}</text>
    </div>
  ))
}

function FocusableRows({ inputIndex = 0 }: { inputIndex?: number }) {
  const [value, setValue] = React.useState("")
  return (
    <virtual-list
      overdraw={0}
      estimatedItemHeight={40}
      style={{ width: 400, height: 160 }}
    >
      {Array.from({ length: 30 }, (_, index) => (
        <div key={index} style={{ height: 40, flexShrink: 0 }}>
          {index === inputIndex ? (
            <input
              autoFocus
              placeholder="focused-input"
              value={value}
              onChange={(event) => setValue(event.value ?? "")}
            />
          ) : (
            <text>{`row-${index}`}</text>
          )}
        </div>
      ))}
    </virtual-list>
  )
}

function DynamicFocusableRows({ enabled }: { enabled: boolean }) {
  const [value, setValue] = React.useState("")
  return (
    <virtual-list
      overdraw={0}
      estimatedItemHeight={40}
      style={{ width: 400, height: 160 }}
    >
      {Array.from({ length: 30 }, (_, index) => (
        <div key={index} style={{ height: 40, flexShrink: 0 }}>
          {index === 0 && enabled ? (
            <input
              autoFocus
              value={value}
              onChange={(event) => setValue(event.value ?? "")}
            />
          ) : (
            <text>{`row-${index}`}</text>
          )}
        </div>
      ))}
    </virtual-list>
  )
}

describe("<virtual-list>", () => {
  it("builds and paints only rows near the viewport", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={100} />
      </virtual-list>
    )

    expect(renderer.getAllText()).toHaveLength(100)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("row-0")
    expect(painted).not.toContain("row-99")
    expect(painted.length).toBeLessThan(10)
  })

  it("builds a distant row when it is scrolled into view", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={100} />
      </virtual-list>
    )

    const list = renderer.findByType("virtual-list")[0]
    expect(list.children).toHaveLength(100)
    renderer.scrollToItem(list.id, 99)
    expect(renderer.getScrollOffset(list.id)?.[1]).toBeLessThan(-100)

    const painted = renderer.getPaintedText()
    expect(painted).toContain("row-99")
    expect(painted).not.toContain("row-0")
  })

  // The anchor survives a prepend because splice_focusable shifts it, but a
  // reader anchored on a row that content is spliced UNDER (a loading row)
  // needs the pixel-preserving form: a negative offset anchors the viewport
  // top above the item and gpui resolves it against measured heights.
  it("scrolls to an item with a pixel offset and reports the logical anchor", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={100} />
      </virtual-list>
    )
    const list = renderer.findByType("virtual-list")[0]

    // The third element is the list's viewport height (the style height).
    renderer.scrollToItem(list.id, 50, 25)
    expect(renderer.getListScrollTop(list.id)).toEqual([50, 25, 160])
    expect(renderer.getPaintedText()).toContain("row-50")

    // 100px above row 50: layout walks up over rows 49..47 (40px each) and
    // lands 20px into row 47.
    renderer.scrollToItem(list.id, 50, -100)
    expect(renderer.getListScrollTop(list.id)).toEqual([47, 20, 160])
    const painted = renderer.getPaintedText()
    expect(painted).toContain("row-47")
    expect(painted).toContain("row-50")

    // Walking past the first row clamps at the very top.
    renderer.scrollToItem(list.id, 1, -400)
    expect(renderer.getListScrollTop(list.id)).toEqual([0, 0, 160])

    // A plain div is not a virtual list and has no logical anchor.
    const row = renderer.findByText("row-0")!
    expect(renderer.getListScrollTop(row.id)).toBeNull()
  })

  it("lazily builds custom elements inside rows", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={80}
        style={{ width: 400, height: 160 }}
      >
        {Array.from({ length: 30 }, (_, index) => (
          <div key={index} style={{ minHeight: 80, flexShrink: 0 }}>
            {index === 20 ? <markdown source="# Lazy markdown" /> : <text>{`row-${index}`}</text>}
          </div>
        ))}
      </virtual-list>
    )

    expect(renderer.findByType("markdown")).toHaveLength(1)
    expect(renderer.getPaintedText()).not.toContain("Lazy markdown")

    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 20)
    expect(renderer.getPaintedText()).toContain("Lazy markdown")
  })

  it("keeps a focused row active when it scrolls offscreen", () => {
    const { render, renderer } = createTestRoot()
    render(<FocusableRows />)

    const input = renderer.findByType("input")[0]
    renderer.simulateKeystrokes("a")
    expect(renderer.getElement(input.id)?.customProps?.value).toBe("a")

    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 29)
    renderer.simulateKeystrokes("b")
    expect(renderer.getElement(input.id)?.customProps?.value).toBe("ab")
  })

  it("reveals an initially focused offscreen row", () => {
    const { render, renderer } = createTestRoot()
    render(<FocusableRows inputIndex={20} />)

    expect(renderer.getPaintedText()).toContain("focused-input")

    const input = renderer.findByType("input")[0]
    renderer.simulateKeystrokes("a")
    expect(renderer.getElement(input.id)?.customProps?.value).toBe("a")
  })

  it("updates focus retention when an existing row becomes focusable", () => {
    const { render, renderer } = createTestRoot()
    render(<DynamicFocusableRows enabled={false} />)
    render(<DynamicFocusableRows enabled />)

    const input = renderer.findByType("input")[0]
    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 29)
    renderer.simulateKeystrokes("a")

    expect(renderer.getElement(input.id)?.customProps?.value).toBe("a")
  })

  it("follows appended chat rows while tail following is active", () => {
    const { render, renderer } = createTestRoot()
    const transcript = (count: number) => (
      <virtual-list
        alignment="bottom"
        followTail
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={count} />
      </virtual-list>
    )

    render(transcript(20))
    expect(renderer.getPaintedText()).toContain("row-19")
    expect(renderer.getPaintedText()).not.toContain("row-0")

    render(transcript(21))
    expect(renderer.getPaintedText()).toContain("row-20")
    expect(renderer.getPaintedText()).not.toContain("row-0")
  })

  it("keeps only the mounted window in the retained tree", () => {
    const { render, renderer } = createTestRoot()
    const windowed = (start: number) => (
      <virtual-list
        itemCount={1000}
        windowStart={start}
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        {Array.from({ length: 8 }, (_, offset) => (
          <div
            key={start + offset}
            style={{
              display: "flex",
              height: 40,
              flexShrink: 0,
              alignItems: "center",
            }}
          >
            <text>{`row-${start + offset}`}</text>
          </div>
        ))}
      </virtual-list>
    )

    render(windowed(0))
    const list = renderer.findByType("virtual-list")[0]
    expect(list.children).toHaveLength(8)
    expect(renderer.getAllText()).toEqual([
      "row-0",
      "row-1",
      "row-2",
      "row-3",
      "row-4",
      "row-5",
      "row-6",
      "row-7",
    ])
    expect(renderer.getPaintedText()).toContain("row-0")

    render(windowed(50))
    expect(renderer.findByType("virtual-list")[0].children).toHaveLength(8)
    expect(renderer.getAllText()).toEqual([
      "row-50",
      "row-51",
      "row-52",
      "row-53",
      "row-54",
      "row-55",
      "row-56",
      "row-57",
    ])

    renderer.scrollToItem(list.id, 50)
    expect(renderer.getPaintedText()).toContain("row-50")
    expect(renderer.getPaintedText()).not.toContain("row-0")
  })

  it("ignores itemCount when estimatedItemHeight is missing", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list itemCount={1000} windowStart={0} style={{ width: 400, height: 160 }}>
        {Array.from({ length: 8 }, (_, index) => (
          <div
            key={index}
            style={{
              display: "flex",
              height: 40,
              flexShrink: 0,
              alignItems: "center",
            }}
          >
            <text>{`row-${index}`}</text>
          </div>
        ))}
      </virtual-list>,
    )

    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 80)

    const painted = renderer.getPaintedText()
    expect(painted.some((line) => line.startsWith("row-"))).toBe(true)
    expect(renderer.getAllText()).toHaveLength(8)
  })

  it("keeps estimated height for logical rows React has not mounted", () => {
    const { render, renderer } = createTestRoot()
    const windowed = (start: number) => (
      <virtual-list
        itemCount={1000}
        windowStart={start}
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        {Array.from({ length: 8 }, (_, offset) => (
          <div
            key={start + offset}
            style={{
              display: "flex",
              height: 40,
              flexShrink: 0,
              alignItems: "center",
            }}
          >
            <text>{`row-${start + offset}`}</text>
          </div>
        ))}
      </virtual-list>
    )

    render(windowed(0))
    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 50)
    renderer.scrollToItem(list.id, 80)

    const offset = renderer.getScrollOffset(list.id)?.[1] ?? 0
    expect(offset).toBeCloseTo(-80 * 40, 0)
  })

  it("paints a newly mounted window after a jump past unmounted rows", () => {
    const { render, renderer } = createTestRoot()
    const windowed = (start: number, rowHeight: number) => (
      <virtual-list
        itemCount={1000}
        windowStart={start}
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        {Array.from({ length: 8 }, (_, offset) => (
          <div
            key={start + offset}
            style={{
              display: "flex",
              height: rowHeight,
              flexShrink: 0,
              alignItems: "center",
            }}
          >
            <text>{`row-${start + offset}`}</text>
          </div>
        ))}
      </virtual-list>
    )

    render(windowed(0, 40))
    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 50)
    render(windowed(50, 80))

    const painted = renderer.getPaintedText()
    expect(painted).toContain("row-50")
    expect(painted).not.toContain("row-0")
    expect(painted.length).toBeLessThan(5)
  })

  // gpui anchors a list on a logical item, so a prepend keeps the rows that are
  // already on screen and pushes the new ones above the viewport. A browser does
  // the same, except that it suppresses scroll anchoring at scrollTop 0. A list
  // pinned to the top must match the browser, or a prepend is never seen.
  //
  // This only bites once the content is taller than the viewport. While it is
  // shorter, gpui re-anchors to item 0 on every layout and hides the drift.
  const grown = (count: number) => (
    <virtual-list
      overdraw={0}
      estimatedItemHeight={40}
      style={{ width: 400, height: 160 }}
    >
      {Array.from({ length: count }, (_, index) => (
        <div
          key={count - index}
          style={{
            display: "flex",
            height: 40,
            flexShrink: 0,
            alignItems: "center",
          }}
        >
          <text>{`row-${count - index}`}</text>
        </div>
      ))}
    </virtual-list>
  )

  it("stays at the top when rows are prepended past the viewport", () => {
    const { render, renderer } = createTestRoot()

    render(grown(2))
    expect(renderer.getPaintedText()[0]).toBe("row-2")

    for (let count = 3; count <= 12; count += 1) {
      render(grown(count))
      expect(renderer.getPaintedText()[0], `after ${count} rows`).toBe(
        `row-${count}`,
      )
    }
  })

  it("keeps following the tail on a short list that is pinned at the top", () => {
    // A following list that does not fill its viewport ends layout anchored at
    // {0, 0}, which reads exactly like "the user is at the top". Pinning it
    // there would call stop_following and break the chat tail.
    const { render, renderer } = createTestRoot()
    const following = (count: number) => (
      <virtual-list
        followTail
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={count} />
      </virtual-list>
    )

    render(following(2))
    render(following(3))
    render(following(12))
    expect(renderer.getPaintedText()).toContain("row-11")
    expect(renderer.getPaintedText()).not.toContain("row-0")
  })

  it("keeps the scroll anchor when rows are prepended below the top", () => {
    const { render, renderer } = createTestRoot()

    render(grown(12))
    const list = renderer.findByType("virtual-list")[0]
    renderer.scrollToItem(list.id, 5)
    expect(renderer.getPaintedText()[0]).toBe("row-7")

    // Away from the top, a prepend must not move the rows under the pointer.
    render(grown(13))
    expect(renderer.getPaintedText()[0]).toBe("row-7")
  })

  describe("scrollbar", () => {
    afterEach(() => {
      delete process.env.GPUIX_SCROLLBARS
    })

    const overflowing = (
      <virtual-list overdraw={0} estimatedItemHeight={40} style={{ width: 400, height: 160 }}>
        {Array.from({ length: 100 }, (_, index) => (
          <div key={index} style={{ width: "100%", height: 40, flexShrink: 0 }}>
            <text>{`row-${index}`}</text>
          </div>
        ))}
      </virtual-list>
    )

    it("a classic bar reserves a gutter, and rows shrink by it", () => {
      process.env.GPUIX_SCROLLBARS = "classic"
      const { render, renderer } = createTestRoot()
      render(overflowing)
      const row = renderer.findByType("div")[0]!
      // The first frame learns that the content overflows, and the
      // second one reserves the gutter.
      renderer.getElementBounds(row.id)
      renderer.getElementBounds(row.id)
      expect(renderer.getElementBounds(row.id)![2]).toBe(385)
    })

    it("a thumb drag scrolls the list", () => {
      process.env.GPUIX_SCROLLBARS = "classic"
      const { render, renderer } = createTestRoot()
      render(overflowing)
      const list = renderer.findByType("virtual-list")[0]!
      const row = renderer.findByType("div")[0]!
      renderer.getElementBounds(row.id)
      renderer.getElementBounds(row.id)
      // 100 rows of 40 in a 160 viewport. The thumb is the 20px floor,
      // at the top of the 15px strip on the right edge. Take it to the
      // middle of the 140px of track room: (80 - 10) / 140 of the
      // 3840px range is 1920.
      renderer.nativeSimulateMouseDown(392, 10)
      renderer.nativeSimulateMouseMove(392, 80, 0)
      renderer.nativeSimulateMouseUp(392, 80)
      expect(renderer.getScrollOffset(list.id)![1]).toBeCloseTo(-1920, 0)
    })
  })

  it("lets overflow-x inside a row pan without moving the list", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={80}
        style={{ width: 240, height: 160 }}
      >
        <div style={{ width: "100%", height: 80, overflowX: "scroll" }}>
          <div style={{ width: 800, height: 80, flexShrink: 0 }}>
            <text>wide row</text>
          </div>
        </div>
        <div style={{ height: 80 }}>
          <text>below</text>
        </div>
      </virtual-list>
    )

    const list = renderer.findByType("virtual-list")[0]
    const scroller = renderer
      .findByType("div")
      .find((d) => d.style.overflowX === "scroll")!
    expect(renderer.getScrollOffset(scroller.id)?.[0] ?? 0).toBe(0)

    renderer.nativeSimulateScrollWheel(80, 40, -80, 0)
    const listOffset = renderer.getScrollOffset(list.id)
    const rowOffset = renderer.getScrollOffset(scroller.id)
    expect(listOffset?.[1] ?? 0, `list ${JSON.stringify(listOffset)}`).toBeCloseTo(0)
    expect(rowOffset?.[0], `row ${JSON.stringify(rowOffset)}`).toBeLessThan(0)
  })

  it("scrolls and keeps selecting when the anchor row unmounts", () => {
    const { render, renderer } = createTestRoot()
    render(
      <virtual-list
        overdraw={0}
        estimatedItemHeight={40}
        style={{ width: 400, height: 160 }}
      >
        <Rows count={30} />
      </virtual-list>,
    )
    const list = renderer.findByType("virtual-list")[0]

    renderer.nativeSimulateMouseDown(1, 20)
    renderer.nativeSimulateMouseMove(1, 158, 0)
    for (let tick = 0; tick < 8; tick += 1) {
      renderer.advanceTime(24)
    }

    const selected = renderer.getSelectedText()
    expect(renderer.getScrollOffset(list.id)?.[1]).toBeLessThan(-40)
    expect(selected).toContain("row-0")
    expect(selected).toMatch(/row-[5-9]/)

    renderer.nativeSimulateMouseUp(1, 158)
    const stoppedAt = renderer.getScrollOffset(list.id)?.[1]
    renderer.advanceTime(48)
    expect(renderer.getScrollOffset(list.id)?.[1]).toBe(stoppedAt)
    expect(renderer.getSelectedText()).toBe(selected)
  })

})
