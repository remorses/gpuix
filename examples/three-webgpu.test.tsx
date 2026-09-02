/// Three.js WebGPURenderer draws a cube into a GPUIX GPUCanvas.
import "./webgpu-polyfill.ts"
import fs from "fs"
import path from "path"
import { fileURLToPath } from "url"
import { describe, expect, it } from "vitest"
import React from "react"
import { BoxGeometry, Color, Mesh, MeshBasicMaterial, PerspectiveCamera, Scene } from "three"
import { WebGPURenderer } from "three/webgpu"
import { createGPUCanvas, installWebGpu } from "@gpuix/react/webgpu"
import { createTestRoot, hasNativeTestRenderer } from "@gpuix/react/testing"

const describeNative = hasNativeTestRenderer ? describe : describe.skip
const shots = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../packages/react/screenshots")

describeNative("three webgpu cube", () => {
  it("inits WebGPURenderer and paints a cube", { timeout: 30_000 }, async () => {
    installWebGpu()
    const canvas = createGPUCanvas(320, 240)
    const scene = new Scene()
    scene.background = new Color(0x11111b)
    const camera = new PerspectiveCamera(50, 320 / 240, 0.1, 100)
    camera.position.set(1.5, 1.2, 3)
    camera.lookAt(0, 0, 0)
    const mesh = new Mesh(
      new BoxGeometry(1, 1, 1),
      new MeshBasicMaterial({ color: 0xf38ba8 }),
    )
    scene.add(mesh)

    const renderer = new WebGPURenderer({ canvas, antialias: false })
    await renderer.init()
    expect(renderer.backend.isWebGPUBackend).toBe(true)
    renderer.setSize(320, 240, false)
    renderer.render(scene, camera)

    const root = createTestRoot({ width: 320, height: 240 })
    fs.mkdirSync(shots, { recursive: true })
    const blankPath = path.join(shots, "gpuix-three-webgpu-cube-blank.png")
    const pngPath = path.join(shots, "gpuix-three-webgpu-cube.png")
    root.render(<canvas style={{ width: 320, height: 240 }} />)
    root.renderer.flush()
    root.renderer.flush()
    if (fs.existsSync(blankPath)) fs.unlinkSync(blankPath)
    root.renderer.captureScreenshot(blankPath)

    root.render(<canvas source={canvas.id} style={{ width: 320, height: 240 }} />)
    root.renderer.flush()
    root.renderer.flush()
    if (fs.existsSync(pngPath)) fs.unlinkSync(pngPath)
    root.renderer.captureScreenshot(pngPath)
    expect(fs.existsSync(pngPath)).toBe(true)
    const pixels = canvas.readPixels()
    expect(pixels.length).toBe(320 * 240 * 4)
    const center = pixels.subarray((120 * 320 + 160) * 4, (120 * 320 + 160) * 4 + 3)
    expect(center[0]).toBeGreaterThan(100)
    const blank = fs.readFileSync(blankPath)
    const painted = fs.readFileSync(pngPath)
    expect(blank.equals(painted)).toBe(false)
    renderer.dispose()
    canvas.destroy()
  })
})
