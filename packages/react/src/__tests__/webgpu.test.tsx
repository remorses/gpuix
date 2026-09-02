/// WebGPU napi + <canvas source> paints a GPU triangle into the GPUI scene.
import fs from "fs"
import { describe, expect, it } from "vitest"
import React from "react"
import { GPUBufferUsage, GPUCanvas, GPUTextureUsage, gpu } from "../webgpu.js"
import { createTestRoot, hasNativeTestRenderer } from "../testing.js"
import { SHOTS_DIR, expectScreenshotsDiffer } from "./test-utils.js"

const describeNative = hasNativeTestRenderer ? describe : describe.skip

const TRIANGLE_WGSL = `
struct VertexOut {
  @builtin(position) position: vec4<f32>,
  @location(0) color: vec4<f32>,
}

@vertex
fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
  var positions = array<vec2<f32>, 3>(
    vec2<f32>(0.0, 0.7),
    vec2<f32>(-0.7, -0.7),
    vec2<f32>(0.7, -0.7),
  );
  var colors = array<vec3<f32>, 3>(
    vec3<f32>(1.0, 0.2, 0.3),
    vec3<f32>(0.2, 1.0, 0.4),
    vec3<f32>(0.2, 0.4, 1.0),
  );
  var out: VertexOut;
  out.position = vec4<f32>(positions[index], 0.0, 1.0);
  out.color = vec4<f32>(colors[index], 1.0);
  return out;
}

@fragment
fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
  return input.color;
}
`

describeNative("webgpu canvas", () => {
  it("creates a GPU device and paints a triangle", async () => {
    const adapter = await gpu.requestAdapter()
    const device = await adapter.requestDevice()
    const canvas = new GPUCanvas(256, 256)
    const context = canvas.getContext("webgpu")
    const format = gpu.getPreferredCanvasFormat()
    context.configure({
      device,
      format,
      usage: GPUTextureUsage.RENDER_ATTACHMENT | GPUTextureUsage.COPY_SRC,
    })

    const shader = device.createShaderModule({ code: TRIANGLE_WGSL })
    const pipeline = device.createRenderPipeline({
      layout: "auto",
      vertex: { module: shader, entryPoint: "vs_main" },
      fragment: {
        module: shader,
        entryPoint: "fs_main",
        targets: [{ format }],
      },
    })
    const texture = context.getCurrentTexture()
    const encoder = device.createCommandEncoder()
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: texture.createView(),
          loadOp: "clear",
          storeOp: "store",
          clearValue: { r: 1, g: 0, b: 1, a: 1 },
        },
      ],
    })
    pass.setPipeline(pipeline)
    pass.draw(3)
    pass.end()
    device.queue.submit([encoder.finish()])

    expect(canvas.id).toBeGreaterThan(0)
    expect(GPUBufferUsage.VERTEX).toBeGreaterThan(0)

    const root = createTestRoot({ width: 256, height: 256 })
    const blankPath = `${SHOTS_DIR}/gpuix-webgpu-triangle-blank.png`
    const path = `${SHOTS_DIR}/gpuix-webgpu-triangle.png`
    root.render(<canvas style={{ width: 256, height: 256 }} />)
    root.renderer.flush()
    root.renderer.flush()
    if (fs.existsSync(blankPath)) fs.unlinkSync(blankPath)
    root.renderer.captureScreenshot(blankPath)

    root.render(
      <canvas source={canvas.id} style={{ width: 256, height: 256 }} />,
    )
    root.renderer.flush()
    root.renderer.flush()
    if (fs.existsSync(path)) fs.unlinkSync(path)
    root.renderer.captureScreenshot(path)
    expect(fs.existsSync(path)).toBe(true)
    const pixels = canvas.readPixels()
    const at = (x: number, y: number) =>
      Array.from(pixels.subarray((y * 256 + x) * 4, (y * 256 + x) * 4 + 4))
    expect(at(4, 4)).toEqual([255, 0, 255, 255])
    expect(at(128, 40)).not.toEqual([255, 0, 255, 255])
    const canvases = root.renderer.findByType("canvas")
    expect(canvases.length).toBe(1)
    expect(canvases[0]?.customProps?.source).toBe(canvas.id)
    expectScreenshotsDiffer(blankPath, path)
    canvas.destroy()
  })
})
