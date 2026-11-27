/// In-process Playwright-like automation against the real GPU test renderer.

import fs from "fs"
import os from "os"
import path from "path"
import React, { useState } from "react"
import { describe, expect, it } from "vitest"
import { connectTest } from "../automation/index.js"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

function Counter() {
  const [count, setCount] = useState(0)
  return (
    <div style={{ width: 400, height: 200 }}>
      <div
        testId="inc"
        style={{ width: 200, height: 80 }}
        onClick={() => setCount((value) => value + 1)}
      >
        <text>{`Count: ${count}`}</text>
      </div>
    </div>
  )
}

describeNative("automation", () => {
  it("returns unique descendants in tree order across overlapping locator roots", async () => {
    const { render, renderer } = createTestRoot()
    render(
      <div testId="outer">
        <div testId="inner">
          <div testId="deep"><text>one target</text></div>
          <text>second target</text>
        </div>
        <div><text>third target</text></div>
        <text>outside</text>
      </div>
    )
    const app = await connectTest(renderer)
    const target = app.getByType("div").getByText("one target")
    const id = renderer.findByText("one target")!.id
    expect(await target.count()).toBe(1)
    expect((await target.all()).map((node) => node.id)).toEqual([id])
    expect((await target.element()).id).toBe(id)
    expect((await target.waitFor()).id).toBe(id)

    const targets = app.getByType("div").getByText("target")
    expect((await targets.all()).map((node) => node.text)).toEqual([
      "one target", "second target", "third target",
    ])
    await expect(targets.element()).rejects.toMatchObject({ code: "Ambiguous" })
    const nested = app.getByType("div").getByType("div").getByText("target")
    expect((await nested.all()).map((node) => node.text)).toEqual([
      "one target", "second target", "third target",
    ])
    expect(await app.getByTestId("outer").getByTestId("outer").count()).toBe(0)
    expect(await app.getByTestId("inner").getByText("outside").count()).toBe(0)
    await app.close()
  })

  it("clicks a testId locator and waits for text", async () => {
    const { render, renderer } = createTestRoot()
    render(<Counter />)
    const app = await connectTest(renderer)

    expect(await app.getByText("Count: 0").textContent()).toBe("Count: 0")
    await app.getByTestId("inc").click()
    await app.getByText("Count: 1").waitFor()
    expect(renderer.getAllText()).toEqual(["Count: 1"])
    await app.close()
  })

  it("captures review frames at frozen clock times", async () => {
    function Fade() {
      return (
        <div
          testId="box"
          style={{ width: 200, height: 80, backgroundColor: "#1e1e2e" }}
          motion={{
            initial: { opacity: 0 },
            animate: { opacity: 1 },
            transition: { duration: 0.3, ease: "linear" },
          }}
        >
          <text>box</text>
        </div>
      )
    }

    const { render, renderer } = createTestRoot()
    render(<Fade />)
    const app = await connectTest(renderer)
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), "gpuix-automation-"))
    const frames = await app.captureFrames(dir, [0, 300])
    expect(frames).toHaveLength(2)
    expect(fs.statSync(frames[0]).size).toBeGreaterThan(0)
    expect(fs.statSync(frames[1]).size).toBeGreaterThan(0)
    await app.close()
  })

  it("drags a locator through interpolated moves", async () => {
    const log: string[] = []

    function Draggable() {
      const [x, setX] = useState(20)
      const [origin, setOrigin] = useState<{ pointer: number; box: number } | null>(
        null
      )
      return (
        <div style={{ width: 600, height: 200, position: "relative" }}>
          <div
            testId="handle"
            style={{
              position: "absolute",
              left: x,
              top: 40,
              width: 80,
              height: 40,
              backgroundColor: "#3366ff",
            }}
            onMouseDown={(event) => {
              log.push("down")
              setOrigin({ pointer: event.x ?? 0, box: x })
            }}
            onMouseMove={(event) => {
              if (!origin) return
              log.push("move")
              setX(origin.box + (event.x ?? 0) - origin.pointer)
            }}
            onMouseUp={() => {
              log.push("up")
              setOrigin(null)
            }}
          >
            <text>{`x=${Math.round(x)}`}</text>
          </div>
        </div>
      )
    }

    const { render, renderer } = createTestRoot()
    render(<Draggable />)
    const app = await connectTest(renderer)

    await app.getByTestId("handle").dragBy(200, 0, { steps: 4 })

    expect(log).toEqual(["down", "move", "move", "move", "move", "up"])
    expect(renderer.getAllText()).toEqual(["x=220"])

    const bounds = await app.getByTestId("handle").bounds()
    expect(Math.round(bounds.x)).toBe(220)
    await app.close()
  })

  it("sends the button a click asks for", async () => {
    const seen: Array<{ button?: number; click?: boolean; aux?: boolean }> = []

    function Target() {
      return (
        <div
          testId="target"
          style={{ width: 200, height: 80, backgroundColor: "#101010" }}
          onMouseDown={(event) => seen.push({ button: event.button })}
          onClick={(event) => seen.push({ click: event.isRightClick })}
          onAuxClick={(event) => seen.push({ aux: event.isRightClick })}
        >
          <text>target</text>
        </div>
      )
    }

    const { render, renderer } = createTestRoot()
    render(<Target />)
    const app = await connectTest(renderer)

    await app.getByTestId("target").click()
    await app.getByTestId("target").click({ button: 2 })

    // `onClick` is the primary button only, like the DOM. A right click
    // reaches `onMouseDown` and `onAuxClick`.
    expect(seen).toEqual([
      { button: 0 },
      { click: false },
      { button: 2 },
      { aux: true },
    ])
    await app.close()
  })

  it("wheels over a locator and reports held modifiers", async () => {
    const seen: Array<{ deltaY: number; cmd: boolean }> = []

    function Surface() {
      return (
        <div
          testId="surface"
          style={{ width: 300, height: 200, backgroundColor: "#101010" }}
          onScroll={(event) =>
            seen.push({
              deltaY: event.deltaY ?? 0,
              cmd: event.modifiers?.cmd ?? false,
            })
          }
        >
          <text>surface</text>
        </div>
      )
    }

    const { render, renderer } = createTestRoot()
    render(<Surface />)
    const app = await connectTest(renderer)

    await app.getByTestId("surface").wheel(0, -60)
    await app.getByTestId("surface").wheel(0, -60, { modifiers: "cmd" })

    expect(seen).toEqual([
      { deltaY: -60, cmd: false },
      { deltaY: -60, cmd: true },
    ])
    await app.close()
  })

  // Below-the-fold content exists in the tree but its painted box lies
  // outside the viewport, so `click()` misses and hover/wheel land nowhere.
  // `locator.scrollIntoView()` walks the nearest scroll parent, like the
  // host-ref `scrollIntoView()`.
  it("scrolls a below-the-fold locator into view", async () => {
    const { render, renderer } = createTestRoot()
    render(
      <div
        testId="scroller"
        style={{ width: 200, height: 100, overflow: "scroll" }}
      >
        <div testId="pad" style={{ height: 200 }} />
        <div testId="target" style={{ height: 40, width: 100 }}>
          <text>deep content</text>
        </div>
      </div>
    )
    const app = await connectTest(renderer)

    const scrollerId = renderer.findByTestId("scroller")!.id
    expect(renderer.getScrollOffset(scrollerId)).toEqual([0, 0])
    await app.getByTestId("target").scrollIntoView()
    expect(renderer.getScrollOffset(scrollerId)![1]).toBeLessThan(0)
    await app.close()
  })

  // Custom elements paint themselves, so they only appear in the bounds
  // registry if their builder attaches `automation::bounds_tracker`. Without
  // it, `click()` on an editor fails with "Element has no painted bounds" and
  // the only workaround is a hard-coded pixel coordinate.
  it("gives an input and a textarea painted bounds", async () => {
    function Form() {
      const [single, setSingle] = useState("one")
      const [multi, setMulti] = useState("two")
      return (
        <div style={{ display: "flex", flexDirection: "column", width: 400, height: 200 }}>
          <input
            testId="single"
            style={{ width: 300, height: 40 }}
            value={single}
            onChange={(event) => setSingle(event.value)}
          />
          <textarea
            testId="multi"
            style={{ width: 300, height: 60 }}
            value={multi}
            onChange={(event) => setMulti(event.value)}
          />
        </div>
      )
    }

    const { render, renderer } = createTestRoot()
    render(<Form />)
    const app = await connectTest(renderer)

    const single = await app.getByTestId("single").bounds()
    const multi = await app.getByTestId("multi").bounds()
    expect(single).not.toBeNull()
    expect(multi).not.toBeNull()
    expect(single!.width).toBeGreaterThan(0)
    expect(single!.height).toBeGreaterThan(0)
    // The textarea is laid out under the input, so its box must start lower.
    expect(multi!.y).toBeGreaterThan(single!.y)

    await app.close()
  })
})
