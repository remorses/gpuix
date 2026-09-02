/**
 * GPUIX Three.js WebGPU cube.
 *
 * Draws a rotating cube with three/webgpu into a GPUCanvas, then composites
 * that canvas into the GPUI window.
 */
import "./webgpu-polyfill.ts"
import React, { useEffect, useState } from "react"
import { BoxGeometry, Color, Mesh, MeshBasicMaterial, PerspectiveCamera, Scene } from "three"
import { WebGPURenderer } from "three/webgpu"
import { render } from "@gpuix/react"
import { createGPUCanvas, installWebGpu } from "@gpuix/react/webgpu"

installWebGpu()

function CubeApp() {
  const [canvas] = useState(() => createGPUCanvas(640, 480))

  useEffect(() => {
    let disposed = false
    let frame = 0
    let webgpu: WebGPURenderer | undefined
    ;(async () => {
      const scene = new Scene()
      scene.background = new Color(0x11111b)
      const camera = new PerspectiveCamera(50, 640 / 480, 0.1, 100)
      camera.position.z = 3
      const mesh = new Mesh(
        new BoxGeometry(1, 1, 1),
        new MeshBasicMaterial({ color: 0xf38ba8 }),
      )
      scene.add(mesh)

      const renderer = new WebGPURenderer({
        canvas,
        antialias: false,
      })
      await renderer.init()
      webgpu = renderer
      if (disposed) {
        renderer.dispose()
        return
      }
      renderer.setSize(640, 480, false)

      const tick = () => {
        if (disposed) return
        mesh.rotation.x += 0.012
        mesh.rotation.y += 0.018
        renderer.render(scene, camera)
        frame = requestAnimationFrame(tick)
      }
      tick()
    })().catch((error) => {
      console.error(error)
    })
    return () => {
      disposed = true
      cancelAnimationFrame(frame)
      webgpu?.dispose()
      canvas.destroy()
    }
  }, [canvas])

  return (
    <div
      style={{
        width: "100%",
        height: "100%",
        backgroundColor: "#11111b",
        alignItems: "center",
        justifyContent: "center",
      }}
    >
      <canvas source={canvas.id} style={{ width: 640, height: 480 }} />
    </div>
  )
}

render(<CubeApp />, {
  title: "GPUIX Three.js WebGPU",
  width: 800,
  height: 600,
  focus: process.env.GPUIX_BACKGROUND !== "1",
})
