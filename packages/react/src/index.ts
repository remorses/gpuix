// GPUIX React - React bindings for GPUI
export { createRoot, flushSync } from "./reconciler"
export { createRenderer } from "./reconciler/renderer"
export { GpuixContext, useGpuix, useGpuixRequired } from "./hooks/use-gpuix"
export { useWindowSize } from "./hooks/use-window-size"
export type { Root } from "./reconciler/renderer"
export type { WindowSize } from "./hooks/use-window-size"

// Re-export types
export type { StyleDesc, NativeRenderer } from "./types/host"
export { resetIdCounter } from "./reconciler/host-config"
export { handleGpuixEvent } from "./reconciler/event-registry"

// Testing utilities
export { TestRenderer, createTestRoot } from "./testing"
export type { TestRoot, TestElement } from "./testing"
export type {
  EventPayload,
  EventModifiers,
  WindowOptions,
  WindowSize as NativeWindowSize,
} from "@gpuix/native"

export { GpuixRenderer } from "@gpuix/native"
