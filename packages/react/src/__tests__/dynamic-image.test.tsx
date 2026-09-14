import fs from "node:fs"
import { PNG } from "pngjs"
import React, { useEffect, useLayoutEffect, useRef } from "react"
import { describe, expect, it, vi } from "vitest"
import { GpuixRenderer } from "@gpuix/native"
import { useGpuix, type NativeRenderer } from "../index"
import { createTestRoot, hasNativeTestRenderer } from "../testing"
import { SHOTS_DIR } from "./test-utils"

const describeNative = hasNativeTestRenderer ? describe : describe.skip
const red = () => new Uint8Array([0, 0, 255, 255])
const imageSource = (color: string) => `data:image/svg+xml,${encodeURIComponent(
  `<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><rect width="4" height="4" fill="${color}"/></svg>`,
)}`
const src = imageSource("#0000ff")

function shot(renderer: ReturnType<typeof createTestRoot>["renderer"], name: string) {
  const path = `${SHOTS_DIR}/dynamic-image-${name}.png`
  renderer.captureScreenshot(path)
  const image = PNG.sync.read(fs.readFileSync(path))
  const target = renderer.findByTestId("image")!
  const [left, top, width, height] = renderer.getElementBounds(target.id)!
  const viewport = renderer.getWindowSize()
  const scaleX = image.width / viewport.width
  const scaleY = image.height / viewport.height
  // Sample the element, not the window: platforms can clamp the requested window size.
  // Stay inside each texel field, away from GPUI's linear atlas edges.
  return [0.375, 0.5, 0.625].flatMap(y => [0.375, 0.5, 0.625].map(x => {
    const pixelX = Math.floor((left! + x * width!) * scaleX)
    const pixelY = Math.floor((top! + y * height!) * scaleY)
    expect(pixelX).toBeGreaterThanOrEqual(0)
    expect(pixelX).toBeLessThan(image.width)
    expect(pixelY).toBeGreaterThanOrEqual(0)
    expect(pixelY).toBeLessThan(image.height)
    const offset = (pixelY * image.width + pixelX) * 4
    return Array.from(image.data.subarray(offset, offset + 4))
  }))
}

function expectColor(samples: number[][], rgba: number[]) {
  for (const pixel of samples) {
    pixel.forEach((channel, i) => expect(Math.abs(channel - rgba[i]!)).toBeLessThanOrEqual(2))
  }
}

describeNative("dynamic image public path", () => {
  it.each([
    ["layout", 64, 0],
    ["passive", 64, 0],
    ["layout", 256, 24],
    ["passive", 256, 24],
  ] as const)("uploads from a %s effect in a %ipx window at offset %i", (timing, windowSize, offset) => {
    const red = () => new Uint8Array(Array.from({ length: 16 }, () => [0, 0, 255, 255]).flat())
    const blue = () => new Uint8Array(Array.from({ length: 16 }, () => [255, 0, 0, 255]).flat())
    const root = createTestRoot({ width: windowSize, height: windowSize })
    const commits = vi.spyOn(root.renderer, "applyBatch")
    let renderer!: NativeRenderer
    let id = -1
    let renders = 0
    const useUploadEffect = timing === "layout" ? useLayoutEffect : useEffect
    function App({ source = src, upload = true }: { source?: string; upload?: boolean }) {
      renders++
      renderer = useGpuix().renderer!
      const ref = useRef<{ id: number }>(null)
      useUploadEffect(() => {
        id = ref.current!.id
        if (upload) renderer.updateImage!(id, 4, 4, red())
      }, [source, upload])
      return <img ref={ref} testId="image" src={source} style={{ position: "absolute", left: offset, top: offset, width: 64, height: 64 }} />
    }
    root.render(<App />)
    expect(id).toBeGreaterThanOrEqual(0)
    expect(root.renderer.findByType("img")).toHaveLength(1)
    const redShot = shot(root.renderer, `${timing}-red`)
    expectColor(redShot, [255, 0, 0, 255])
    const commitCount = commits.mock.calls.length
    const renderCount = renders
    const tree = root.renderer.toJSON()

    // Only the offset typed-array view is copied; mutating it after return is safe.
    const backing = new Uint8Array([99, 99, ...red(), 77, 77])
    renderer.updateImage!(id, 4, 4, backing.subarray(2, 66))
    backing.fill(0)
    expectColor(shot(root.renderer, `${timing}-copied`), [255, 0, 0, 255])
    renderer.updateImage!(id, 4, 4, blue())
    expectColor(shot(root.renderer, `${timing}-blue`), [0, 0, 255, 255])
    renderer.updateImage!(id, 4, 4, blue())
    renderer.updateImage!(id, 4, 4, Buffer.from(red()))
    expectColor(shot(root.renderer, `${timing}-latest`), [255, 0, 0, 255])
    expect(commits).toHaveBeenCalledTimes(commitCount)
    expect(renders).toBe(renderCount)
    expect(root.renderer.toJSON()).toEqual(tree)

    // Same src across a React commit preserves pixels; a real src change takes over.
    root.render(<App upload={false} />)
    expectColor(shot(root.renderer, `${timing}-same-src`), [255, 0, 0, 255])
    root.render(<App source={imageSource("#00ff00")} upload={false} />)
    root.renderer.flush()
    root.renderer.flush()
    const sourceShot = shot(root.renderer, `${timing}-src`)
    expectColor(sourceShot, [0, 255, 0, 255])
    renderer.updateImage!(id, 4, 4, red())
    renderer.clearImage!(id)
    renderer.clearImage!(id)
    expectColor(shot(root.renderer, `${timing}-cleared`), [0, 255, 0, 255])

    // An effect on the next commit wins after that commit's src mutation.
    root.render(<App source={imageSource("#ffffff")} />)
    const nextRed = shot(root.renderer, `${timing}-next-effect`)
    expect(nextRed).not.toEqual(sourceShot)
    expectColor(nextRed, [255, 0, 0, 255])
    root.unmount()
    root.renderer.flush()
    expect(root.renderer.findByType("img")).toHaveLength(0)
    expect(() => renderer.updateImage!(id, 4, 4, red())).toThrow(/existing img/)
    expect(() => renderer.clearImage!(id)).toThrow(/existing img/)
  })

  it("remeasures intrinsic image height inside a virtualized row on resize", () => {
    const root = createTestRoot({ width: 64, height: 64 })
    root.render(
      <virtual-list style={{ width: 64, height: 64 }}>
        <div><img testId="resizing" style={{ width: 32 }} /></div>
        <div testId="following" style={{ height: 8 }} />
      </virtual-list>,
    )
    const renderer = root.renderer
    const id = renderer.findByTestId("resizing")!.id
    const following = renderer.findByTestId("following")!.id
    renderer.updateImage(id, 1, 1, red())
    renderer.flush()
    expect(renderer.getElementBounds(id)![3]).toBeCloseTo(32)
    const before = renderer.getElementBounds(following)![1]
    renderer.updateImage(id, 2, 1, new Uint8Array([...red(), ...red()]))
    renderer.flush()
    expect(renderer.getElementBounds(id)![3]).toBeCloseTo(16)
    expect(renderer.getElementBounds(following)![1]).toBeCloseTo(before - 16)
    root.unmount()
  })

  it("rejects invalid native-boundary input without changing the image", () => {
    const root = createTestRoot({ width: 64, height: 64 })
    root.render(<div><img testId="image" style={{ width: 64, height: 64 }} /></div>)
    const renderer = root.renderer
    const id = renderer.findByTestId("image")!.id
    renderer.updateImage(id, 1, 1, red())
    const before = shot(renderer, "errors-before")
    for (const dimension of [0, -1, 1.5, NaN, Infinity, 2 ** 32]) {
      expect(() => renderer.updateImage(id, dimension, 1, red())).toThrow(/dimensions/)
      expect(() => renderer.updateImage(id, 1, dimension, red())).toThrow(/dimensions/)
    }
    for (const [width, height] of [[4097, 1], [1, 4097], [1_000_000, 1]]) {
      expect(() => renderer.updateImage(id, width!, height!, new Uint8Array())).toThrow(/4096/)
    }
    for (const length of [0, 3, 5, 8]) {
      expect(() => renderer.updateImage(id, 1, 1, new Uint8Array(length))).toThrow(/byte length/)
    }
    expect(() => renderer.updateImage(id, 2 ** 32 - 1, 2 ** 32 - 1, red())).toThrow(/overflow/)
    for (const bad of [-1, 0.5, NaN, Infinity, Number.MAX_SAFE_INTEGER + 1]) {
      expect(() => renderer.updateImage(bad, 1, 1, red())).toThrow(/safe integer/)
      expect(() => renderer.clearImage(bad)).toThrow(/safe integer/)
    }
    expect(() => renderer.updateImage(renderer.findByType("div")[0]!.id, 1, 1, red())).toThrow(/existing img/)
    expect(() => renderer.updateImage(id, 1, 1, new Uint16Array(2) as unknown as Uint8Array)).toThrow()
    expect(shot(renderer, "errors-after")).toEqual(before)
    root.unmount()
  })

  it("rejects stale renderer ownership and uninitialized production renderers", () => {
    const old = createTestRoot()
    old.render(<img testId="old" />)
    const id = old.renderer.findByTestId("old")!.id
    const current = createTestRoot()
    current.render(<img />)
    expect(() => old.renderer.updateImage(id, 1, 1, red())).toThrow(/no longer owns/)
    expect(() => old.renderer.clearImage(id)).toThrow(/no longer owns/)
    const production = new GpuixRenderer()
    expect(() => production.updateImage(id, 1, 1, red())).toThrow(/not initialized/)
    expect(() => production.clearImage(id)).toThrow(/not initialized/)
    current.unmount()
  })
})
