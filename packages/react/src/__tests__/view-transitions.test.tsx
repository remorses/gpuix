/**
 * View transitions. The automation clock is paused, so every frame of the
 * animation is read at an exact time. Bounds come from the paint trackers,
 * which record where an element really painted, moved or not.
 */
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import React from "react"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing"
import { startViewTransition, type ViewTransitionOptions } from "../view-transitions"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

/** The `key` makes each screen its own element, the way a navigation swaps
 *  one screen component for another. Without it, React updates one element
 *  in place, which is the separate case the last test covers. */
function Screen({ label, color }: { label: string; color: string }) {
  return (
    <div
      key={label}
      style={{
        width: 300,
        height: 200,
        backgroundColor: color,
        viewTransitionName: "screen",
      }}
    >
      <text>{label}</text>
    </div>
  )
}

/** The iOS push: the new screen slides in from the right over the old one,
 *  and the old one slides a third of the way out to the left. */
const PUSH: ViewTransitionOptions = {
  groups: {
    screen: {
      duration: 0.3,
      ease: "linear",
      old: { translateX: ["0%", "-30%"] },
      new: { translateX: ["100%", "0%"] },
    },
  },
}

describeNative("view transitions", () => {
  let root: TestRoot
  beforeEach(() => {
    root = createTestRoot()
  })
  afterEach(() => {
    root.unmount()
  })

  const screenId = () => root.renderer.findByType("div")[0]!.id
  const boundsOf = (id: number) => root.renderer.getElementBounds(id)

  it("slides the pair like an iOS push", () => {
    const { render, renderer } = root
    renderer.clockPause()
    render(<Screen label="A" color="#ff0000" />)
    const oldId = screenId()
    const baseX = boundsOf(oldId)![0]

    startViewTransition(renderer, () => render(<Screen label="B" color="#00ff00" />), PUSH)
    const newId = screenId()
    expect(newId).not.toBe(oldId)

    // At the start, the new screen sits one width to the right, and the
    // frozen copy of the old one still paints at its place.
    expect(boundsOf(newId)![0]).toBeCloseTo(baseX + 300, 0)
    expect(boundsOf(oldId)![0]).toBeCloseTo(baseX, 0)

    // Halfway, with a linear ease: the new screen covered half its way in,
    // and the old copy moved 15% of its width out.
    renderer.clockFastForward(150)
    expect(boundsOf(newId)![0]).toBeCloseTo(baseX + 150, 0)
    expect(boundsOf(oldId)![0]).toBeCloseTo(baseX - 45, 0)

    // Past the end: the new screen rests at its layout place, and the copy
    // paints no more.
    renderer.clockFastForward(400)
    expect(boundsOf(newId)![0]).toBeCloseTo(baseX, 0)
    expect(boundsOf(oldId)).toBeNull()
  })

  it("crossfades by default without moving anything", () => {
    const { render, renderer } = root
    renderer.clockPause()
    render(<Screen label="A" color="#ff0000" />)
    const oldId = screenId()
    const baseX = boundsOf(oldId)![0]

    startViewTransition(renderer, () => render(<Screen label="B" color="#00ff00" />))
    const newId = screenId()

    renderer.clockFastForward(150)
    expect(boundsOf(newId)![0]).toBeCloseTo(baseX, 0)
    expect(boundsOf(oldId)![0]).toBeCloseTo(baseX, 0)

    renderer.clockFastForward(400)
    expect(boundsOf(oldId)).toBeNull()
  })

  it("animates a name that enters without a captured pair", () => {
    const { render, renderer } = root
    renderer.clockPause()
    render(<div style={{ width: 300, height: 200 }} />)

    startViewTransition(
      renderer,
      () => render(<Screen label="B" color="#00ff00" />),
      { groups: { screen: { duration: 0.3, ease: "linear", new: { translateY: ["100%", "0%"] } } } }
    )
    const id = screenId()
    const baseY = 200 * 1.0

    // The screen is 200 high, so it starts one height down and slides up.
    expect(boundsOf(id)![1]).toBeCloseTo(baseY, 0)
    renderer.clockFastForward(150)
    expect(boundsOf(id)![1]).toBeCloseTo(baseY / 2, 0)
    renderer.clockFastForward(400)
    expect(boundsOf(id)![1]).toBeCloseTo(0, 0)
  })

  it("paints an exit copy for a name that leaves without a successor", () => {
    const { render, renderer } = root
    renderer.clockPause()
    render(<Screen label="A" color="#ff0000" />)
    const oldId = screenId()
    const baseX = boundsOf(oldId)![0]

    // The next tree has no element with the name. The frozen copy paints
    // over the tree and the `old` side slides and blurs it out.
    startViewTransition(renderer, () => render(<div style={{ width: 300, height: 200 }} />), {
      groups: {
        screen: {
          duration: 0.3,
          ease: "linear",
          old: { translateX: ["0%", "100%"], opacity: [1, 0], blur: [0, 6] },
        },
      },
    })

    expect(boundsOf(oldId)![0]).toBeCloseTo(baseX, 0)
    renderer.clockFastForward(150)
    expect(boundsOf(oldId)![0]).toBeCloseTo(baseX + 150, 0)
    renderer.clockFastForward(400)
    expect(boundsOf(oldId)).toBeNull()
  })

  it("a fresh start replaces a running transition", () => {
    const { render, renderer } = root
    renderer.clockPause()
    render(<Screen label="A" color="#ff0000" />)
    const baseX = boundsOf(screenId())![0]

    startViewTransition(renderer, () => render(<Screen label="B" color="#00ff00" />), PUSH)
    renderer.clockFastForward(150)

    // Start again mid-flight. The second transition captures the moved pair
    // and runs on its own clock from here.
    startViewTransition(renderer, () => render(<Screen label="C" color="#0000ff" />), PUSH)
    const thirdId = screenId()
    expect(boundsOf(thirdId)![0]).toBeCloseTo(baseX + 300, 0)
    renderer.clockFastForward(500)
    expect(boundsOf(thirdId)![0]).toBeCloseTo(baseX, 0)
  })

  it("transitions an element React updates in place", () => {
    const { render, renderer } = root
    renderer.clockPause()
    // No keys: React keeps the element and only swaps its style. The frozen
    // copy takes a fresh id, so the live element and the copy never share
    // GPUI element state.
    render(<div style={{ width: 300, height: 200, viewTransitionName: "screen" }} />)
    const id = screenId()

    startViewTransition(renderer, () =>
      render(
        <div style={{ width: 300, height: 100, viewTransitionName: "screen" }} />
      )
    )
    expect(screenId()).toBe(id)
    renderer.clockFastForward(500)
    expect(boundsOf(id)![3]).toBeCloseTo(100, 0)
  })

  it("runs the update alone on a renderer without the native methods", () => {
    let ran = false
    const bare = {} as Parameters<typeof startViewTransition>[0]
    startViewTransition(bare, () => {
      ran = true
    })
    expect(ran).toBe(true)
  })
})
