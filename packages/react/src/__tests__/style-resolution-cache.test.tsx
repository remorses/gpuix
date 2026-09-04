/// The renderer must not resolve a style it already resolved.
///
/// GPUI is immediate mode. It rebuilds the element tree every frame. Without a
/// cache the renderer turns the same unchanged StyleDesc into the same
/// StyleRefinement on every frame, for every element on screen.
///
/// These tests count resolutions instead of measuring wall-clock time. A time
/// budget flakes on a loaded machine, and a flaky gate gets muted. A counter
/// gives an exact number, and it fails loudly when the cache stops working.

import React from "react"
import { describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("style resolution cache", () => {
  it("resolves nothing on a frame that changed nothing", () => {
    const { renderer, render } = createTestRoot()
    render(
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        <div style={{ width: 40, height: 40 }} />
        <div style={{ width: 40, height: 40 }} />
      </div>
    )

    renderer.resetStyleResolutions()
    renderer.flush()
    renderer.flush()
    renderer.flush()

    expect(renderer.styleResolutions()).toBe(0)
  })

  it("resolves nothing on a frame that only advances an animation", () => {
    const { renderer, render } = createTestRoot()

    renderer.clockPause()
    render(
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        <div
          motion={{
            initial: { width: 40 },
            animate: { width: 240 },
            transition: { duration: 1 },
          }}
          style={{ height: 40, backgroundColor: "#ff0000" }}
        />
      </div>
    )

    renderer.resetStyleResolutions()
    renderer.clockFastForward(200)
    renderer.flush()
    renderer.clockFastForward(200)
    renderer.flush()
    renderer.clockResume()

    // A motion frame drives eight numbers onto the element. It used to drive
    // them onto a copy of the whole style and resolve that, which reparsed
    // every declaration the element made on every frame of the animation.
    expect(renderer.styleResolutions()).toBe(0)
  })

  it("resolves one style when one element changes", () => {
    const { renderer, render } = createTestRoot()
    const tree = (color: string) => (
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        <div style={{ width: 40, height: 40, backgroundColor: color }} />
        <div style={{ width: 40, height: 40 }} />
      </div>
    )

    render(tree("#ff0000"))
    renderer.resetStyleResolutions()
    render(tree("#00ff00"))

    expect(renderer.styleResolutions()).toBe(1)
  })

  it("resolves nothing when a re-render sends the same style", () => {
    const { renderer, render } = createTestRoot()
    const tree = (
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        <div style={{ width: 40, height: 40 }} />
      </div>
    )

    render(tree)
    renderer.resetStyleResolutions()
    render(
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        <div style={{ width: 40, height: 40 }} />
      </div>
    )

    expect(renderer.styleResolutions()).toBe(0)
  })

  it("resolves the base style and each variant once per element", () => {
    const { renderer, render } = createTestRoot()

    renderer.resetStyleResolutions()
    render(
      <div
        style={{
          width: 40,
          height: 40,
          backgroundColor: "#111111",
          hover: { backgroundColor: "#222222" },
          active: { backgroundColor: "#333333" },
        }}
      />
    )

    // One base, one hover, one active.
    expect(renderer.styleResolutions()).toBe(3)

    renderer.resetStyleResolutions()
    renderer.flush()
    expect(renderer.styleResolutions()).toBe(0)
  })

  it("keeps the count flat as frames repeat", () => {
    const { renderer, render } = createTestRoot()
    render(
      <div style={{ backgroundColor: "#111111", padding: 8 }}>
        {Array.from({ length: 20 }, (_, i) => (
          <div key={i} style={{ width: 10, height: 10, backgroundColor: "#222222" }} />
        ))}
      </div>
    )

    renderer.resetStyleResolutions()
    for (let i = 0; i < 10; i++) {
      renderer.flush()
    }

    // Ten frames over 21 styled elements. Every one of those 210 resolutions
    // was work the renderer used to repeat.
    expect(renderer.styleResolutions()).toBe(0)
  })
})
