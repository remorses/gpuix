// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler/index.js"
export { createRenderer } from "./reconciler/renderer.js"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix.js"
export { useWindowSize } from "./hooks/use-window-size.js"
export type { Root } from "./reconciler/renderer.js"
export type { WindowSize } from "./hooks/use-window-size.js"

// Re-export types
export type { StyleDesc, NativeRenderer } from "./types/host.js"
export { resetIdCounter } from "./reconciler/host-config.js"
export { handleGpuixEvent } from "./reconciler/event-registry.js"

// Testing utilities
export { TestRenderer, createTestRoot, hasNativeTestRenderer } from "./testing.js"
export type { TestRoot, TestElement } from "./testing.js"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "@gpuix/native"

export { GpuixRenderer } from "@gpuix/native"
