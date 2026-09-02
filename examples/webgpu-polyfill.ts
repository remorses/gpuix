/// three/webgpu reads `self` and requestAnimationFrame at import/init time.
const global = globalThis as {
  self?: typeof globalThis
  requestAnimationFrame?: typeof requestAnimationFrame
  cancelAnimationFrame?: typeof cancelAnimationFrame
}
global.self ??= globalThis
global.requestAnimationFrame ??= ((callback: FrameRequestCallback) => {
  return setTimeout(() => callback(Date.now()), 16) as unknown as number
}) as typeof requestAnimationFrame
global.cancelAnimationFrame ??= ((id: number) => {
  clearTimeout(id)
}) as typeof cancelAnimationFrame
import { installWebGpu } from "@gpuix/react/webgpu"
installWebGpu()
