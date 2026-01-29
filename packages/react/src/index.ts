// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler"
export { createRenderer } from "./reconciler/renderer"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix"
export { useWindowSize } from "./hooks/use-window-size"
export type { Root } from "./reconciler/renderer"
export type { WindowSize } from "./hooks/use-window-size"

// Re-export native types
export type { ElementDesc, StyleDesc } from "./types/host"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "@gpuix/native"

export { GpuixRenderer } from "@gpuix/native"
