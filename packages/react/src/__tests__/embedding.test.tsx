import React, { createRef } from "react"
import { afterEach, beforeEach, describe, expect, it } from "vitest"
import { GpuixRenderer } from "@gpuix/native"
import { flushSync } from "../reconciler/reconciler.js"
import { createTestRoot, hasNativeTestRenderer, type TestRoot } from "../testing.js"
import type { NativeRenderer, PublicInstance } from "../types/host.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

describeNative("native integration snapshots", () => {
  let root: TestRoot
  beforeEach(() => { root = createTestRoot({ width: 300, height: 200 }) })
  afterEach(() => { root.unmount(); root.renderer.flush() })

  it("reports borrowed tagged bytes when the test platform supplies a raw handle", () => {
    const renderer: NativeRenderer = root.renderer
    const handle = renderer.getNativeWindowHandle!()
    if (process.platform === "darwin") expect(handle?.kind).toBe("AppKit")
    if (!handle) return // A headless GPUI platform may report NotSupported.
    expect(["AppKit", "Win32", "Xlib", "Xcb", "Wayland"]).toContain(handle.kind)
    expect(Buffer.isBuffer(handle.handle)).toBe(true)
    expect(handle.handle.length).toBe(handle.kind === "Xcb" || process.arch === "ia32" ? 4 : 8)
    const again = renderer.getNativeWindowHandle!()!
    expect(again).toEqual(handle)
    handle.handle.fill(0)
    expect(renderer.getNativeWindowHandle!()).toEqual(again)
  })

  it("rejects a live query before init rather than leaking a test window", () => {
    const renderer = new GpuixRenderer()
    expect(() => renderer.getNativeWindowHandle()).toThrow()
    expect(() => renderer.getElementPaintState(1)).toThrow()
  })

  it("reports logical bounds, rectangular clipping, and the painted scale", () => {
    const ref = createRef<PublicInstance>()
    root.render(
      <div style={{ width: 100, height: 60, overflow: "hidden", position: "relative" }}>
        <div ref={ref} style={{ position: "absolute", left: 75, top: 10, width: 50, height: 30 }} />
      </div>,
    )
    const state = root.renderer.getElementPaintState(ref.current!.id)!
    expect(state.bounds).toEqual({ x: 75, y: 10, width: 50, height: 30 })
    expect(state.clipBounds).toEqual({ x: 75, y: 10, width: 25, height: 30 })
    expect(state.scaleFactor).toBeGreaterThan(0)
    const { x, y, width, height } = state.bounds
    expect(root.renderer.getElementBounds(ref.current!.id)).toEqual([x, y, width, height])
  })

  it("does not paint on query and drops removed nodes only after the next paint", () => {
    const ref = createRef<PublicInstance>()
    flushSync(() => root.root.render(<div ref={ref} style={{ width: 40, height: 20 }} />))
    const id = ref.current!.id
    expect(root.renderer.getElementPaintState(id)).toBeNull()
    root.renderer.flush()
    const first = root.renderer.getElementPaintState(id)!
    expect(first.bounds.width).toBe(40)

    flushSync(() => root.root.render(<div ref={ref} style={{ width: 80, height: 20 }} />))
    expect(root.renderer.getElementPaintState(id)).toEqual(first)
    root.renderer.flush()
    expect(root.renderer.getElementPaintState(id)!.bounds.width).toBe(80)

    flushSync(() => root.root.render(null))
    expect(root.renderer.getElementPaintState(id)).not.toBeNull()
    root.renderer.flush()
    expect(root.renderer.getElementPaintState(id)).toBeNull()
  })

  it("records leaf geometry and opacity-zero containers without promising pixel visibility", () => {
    const ref = createRef<PublicInstance>()
    root.render(<svg ref={ref} source={'<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect width="40" height="20"/></svg>'} style={{ width: 40, height: 20, color: "#fff" }} />)
    const leafId = ref.current!.id
    expect(root.renderer.getElementPaintState(leafId)!.bounds).toEqual({ x: 0, y: 0, width: 40, height: 20 })
    flushSync(() => root.root.render(null))
    expect(root.renderer.getElementPaintState(leafId)).not.toBeNull()
    root.renderer.flush()
    expect(root.renderer.getElementPaintState(leafId)).toBeNull()
    root.render(<div ref={ref} style={{ width: 40, height: 20, opacity: 0 }} />)
    expect(root.renderer.getElementPaintState(ref.current!.id)).not.toBeNull()
  })

  it("clips to the viewport by default and never reports negative clip dimensions", () => {
    const ref = createRef<PublicInstance>()
    const { width, height } = root.renderer.getWindowSize()
    const left = width - 20
    const top = height - 20
    root.render(<div ref={ref} style={{ position: "absolute", left, top, width: 50, height: 40 }} />)
    const visible = root.renderer.getElementPaintState(ref.current!.id)!
    expect(visible.bounds).toEqual({ x: left, y: top, width: 50, height: 40 })
    expect(visible.clipBounds).toEqual({ x: left, y: top, width: 20, height: 20 })
    root.render(<div ref={ref} style={{ position: "absolute", left: -20, top: -10, width: 50, height: 40 }} />)
    const negative = root.renderer.getElementPaintState(ref.current!.id)!
    expect(negative.bounds).toEqual({ x: -20, y: -10, width: 50, height: 40 })
    expect(negative.clipBounds).toEqual({ x: 0, y: 0, width: 30, height: 30 })
    for (const position of [Math.max(width, height) + 100, -100]) {
      root.render(<div ref={ref} style={{ position: "absolute", left: position, top: position, width: 50, height: 40 }} />)
      const clipped = root.renderer.getElementPaintState(ref.current!.id)
      // GPUI may skip paint altogether; a record is not proof of visibility.
      if (clipped) {
        expect(clipped.clipBounds.width).toBe(0)
        expect(clipped.clipBounds.height).toBe(0)
      }
    }
  })

  it("does not return the most recently painted renderer's geometry to another renderer", () => {
    const ref = createRef<PublicInstance>()
    root.render(<div ref={ref} style={{ width: 40, height: 20 }} />)
    const old = root
    const id = ref.current!.id
    expect(old.renderer.getElementPaintState(id)).not.toBeNull()
    old.unmount()
    root = createTestRoot()
    root.render(<div style={{ width: 90, height: 20 }} />)
    expect(old.renderer.getElementPaintState(id)).toBeNull()
    expect(() => old.renderer.getNativeWindowHandle()).toThrow()
  })

  it("drops virtualized-away rows after scrolling and painting", () => {
    const list = createRef<PublicInstance>()
    const first = createRef<PublicInstance>()
    const last = createRef<PublicInstance>()
    root.render(
      <virtual-list ref={list} overdraw={0} estimatedItemHeight={40} style={{ height: 80 }}>
        {Array.from({ length: 100 }, (_, i) => (
          <div key={i} ref={i === 0 ? first : i === 99 ? last : undefined} style={{ height: 40 }} />
        ))}
      </virtual-list>,
    )
    expect(root.renderer.getElementPaintState(first.current!.id)).not.toBeNull()
    expect(root.renderer.getElementPaintState(last.current!.id)).toBeNull()
    root.renderer.scrollToItem(list.current!.id, 99)
    root.renderer.flush()
    expect(root.renderer.getElementPaintState(first.current!.id)).toBeNull()
    expect(root.renderer.getElementPaintState(last.current!.id)).not.toBeNull()
  })

  it("rejects invalid ids and returns null for an unrecorded id", () => {
    for (const id of [-1, 1.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => root.renderer.getElementPaintState(id)).toThrow()
    }
    expect(root.renderer.getElementPaintState(9999)).toBeNull()
  })
})
