/// `width`, `height` and the four `min`/`max` properties.
///
/// These six read the same CSS lengths as every other length property, plus
/// `auto` and a percentage. A value none of that can read drops on its own and
/// leaves the rest of the style alone.

import fs from "fs"
import path from "path"
import React from "react"
import { beforeAll, describe, expect, it } from "vitest"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import type { StyleDesc } from "../types/host.js"
import { expectScreenshotsEqual, SHOTS_DIR } from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

beforeAll(() => {
  fs.mkdirSync(SHOTS_DIR, { recursive: true })
})

const shot = (name: string) => path.join(SHOTS_DIR, `sizing-${name}.png`)

/// The painted box of the only div in the tree.
function boxOf(style: StyleDesc): [number, number, number, number] {
  const { render, renderer } = createTestRoot()
  render(<div style={style} />)
  const id = renderer.findByType("div")[0]!.id
  const bounds = renderer.getElementBounds(id)
  expect(bounds).toBeTruthy()
  return bounds as [number, number, number, number]
}

const sizeOf = (style: StyleDesc) => boxOf(style).slice(2) as [number, number]

describeNative("sizing", () => {
  it("takes every absolute unit and rem", () => {
    expect(sizeOf({ width: 200, height: 100 })).toEqual([200, 100])
    expect(sizeOf({ width: "200px", height: "100px" })).toEqual([200, 100])
    // A 16 px root, so 6rem is 96 and 1in is 96.
    expect(sizeOf({ width: "6rem", height: "1in" })).toEqual([96, 96])
    expect(sizeOf({ width: "72pt", height: "4pc" })).toEqual([96, 64])
  })

  it("folds arithmetic before layout sees it", () => {
    expect(sizeOf({ width: "calc(100px + 2rem)" })[0]).toBe(132)
    expect(sizeOf({ width: "min(180px, 12rem)" })[0]).toBe(180)
    expect(sizeOf({ width: "clamp(60px, 8rem, 120px)" })[0]).toBe(120)
  })

  it("reads a length through a variable", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ "--spacing": "4px" }}>
        <div style={{ width: "calc(var(--spacing) * 30)", height: "var(--spacing)" }} />
      </div>
    )
    const id = renderer.findByType("div")[1]!.id
    expect(renderer.getElementBounds(id)?.slice(2)).toEqual([120, 4])
  })

  it("still takes a percentage and auto", () => {
    const { render, renderer } = createTestRoot()
    render(
      <div style={{ width: 400, height: 200 }}>
        <div style={{ width: "50%", height: "25%" }} />
      </div>
    )
    const id = renderer.findByType("div")[1]!.id
    expect(renderer.getElementBounds(id)?.slice(2)).toEqual([200, 50])
    // `auto` is a keyword, so the length parser never sees it. It has to land
    // on the size the box takes when nothing declares one.
    expect(sizeOf({ width: "auto", height: "auto" })).toEqual(sizeOf({}))
  })

  it("clamps between min and max written as lengths", () => {
    expect(sizeOf({ width: "10rem", minWidth: "12rem" })[0]).toBe(192)
    expect(sizeOf({ width: "20rem", maxWidth: "calc(100px + 1rem)" })[0]).toBe(116)
    expect(sizeOf({ height: 10, minHeight: "3rem" })[1]).toBe(48)
    expect(sizeOf({ height: 200, maxHeight: "5rem" })[1]).toBe(80)
  })

  it("drops a size it cannot read and keeps the rest of the style", () => {
    // This used to throw out of setStyle and lose every other property in the
    // same commit, so the element painted nothing at all.
    const paint = (name: string, style: StyleDesc) => {
      const root = createTestRoot()
      root.render(<div style={style} />)
      root.renderer.captureScreenshot(shot(name))
      root.unmount()
    }
    for (const bad of ["banana", "3em", "12vw", "var(--missing)"]) {
      paint("dropped", { width: bad, height: 100, backgroundColor: "#ff0000" })
      paint("plain", { height: 100, backgroundColor: "#ff0000" })
      expectScreenshotsEqual(shot("dropped"), shot("plain"))
    }
  })
})
