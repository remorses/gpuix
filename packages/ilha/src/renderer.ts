import { GpuixRenderer } from "@gpuix/native"
import type { EventPayload, WindowOptions } from "@gpuix/native"
import type { NativeRenderer, RootEventHandlers } from "@gpuix/native/host"
import {
  createNativeRenderer,
  installBrowserAutomation,
  startFrameLoop,
  type FrameLoop,
} from "@gpuix/native/runtime"
import type { Component } from "ilha/renderer"

import { createRoot, type Root } from "./root.js"

const RENDER_HOST = Symbol.for("@gpuix/ilha/render-host")

interface RenderSlot {
  renderer?: NativeRenderer
  root?: Root
  loop?: FrameLoop
}

export interface RenderOptions extends WindowOptions, RootEventHandlers {
  /** Use an existing renderer instead of opening a window. */
  renderer?: NativeRenderer
  debugFrameOverlay?: "hidden" | "minimal" | "full"
}

export interface Renderer {
  renderer: GpuixRenderer
  root: Root
}

/** Create the native renderer without opening a window. */
export function createRenderer(
  onEvent?: (event: EventPayload) => void
): GpuixRenderer {
  return createNativeRenderer({
    onError: (error) => console.error("[gpuix-ilha] native event error", error),
    onEvent,
  })
}

function slot(): RenderSlot {
  const global = globalThis as typeof globalThis & {
    [RENDER_HOST]?: RenderSlot
  }
  return (global[RENDER_HOST] ??= {})
}

/**
 * Mount an ilha component into a GPUIX window.
 *
 * A later call replaces the tree on the same window, so `bun --hot` remounts
 * the component without opening a second window.
 */
export function render(
  component: Component,
  options: RenderOptions = {}
): Root {
  const {
    debugFrameOverlay,
    keyboardFocusDim,
    onEvent,
    onKeyDown,
    onKeyUp,
    onSelectionChange,
    onUncaughtError,
    renderer: injected,
    tabNavigation,
    ...windowOptions
  } = options
  const host = slot()
  if (!host.renderer) {
    host.renderer =
      injected ??
      createNativeRenderer({
        onError(error) {
          onUncaughtError?.(
            error instanceof Error ? error : new Error(String(error)),
            {}
          )
        },
      })
    if (!injected && host.renderer instanceof GpuixRenderer) {
      host.renderer.init(windowOptions)
    }
  }
  host.root?.unmount()
  host.root = createRoot(host.renderer, {
    keyboardFocusDim,
    onEvent,
    onKeyDown,
    onKeyUp,
    onSelectionChange,
    onUncaughtError,
    tabNavigation,
  })
  if (!injected && host.renderer instanceof GpuixRenderer && !host.loop) {
    host.loop = startFrameLoop(host.renderer, {
      onError: (error) => console.error("[gpuix-ilha] frame error", error),
      onTerminated: () => {
        if (typeof process !== "undefined") {
          process.exit(0)
        }
      },
    })
  }
  if (
    typeof window !== "undefined" &&
    host.renderer instanceof GpuixRenderer &&
    !Reflect.has(globalThis, "gpuix")
  ) {
    Reflect.set(globalThis, "gpuix", installBrowserAutomation(host.renderer))
  }
  if (debugFrameOverlay) {
    host.renderer.setDebugFrameOverlay?.(debugFrameOverlay)
  }
  host.root.render(component)
  return host.root
}

/** Stop the frame loop, unmount the tree and forget the window. */
export function resetRender(): void {
  const global = globalThis as typeof globalThis & {
    [RENDER_HOST]?: RenderSlot
  }
  const host = global[RENDER_HOST]
  host?.loop?.stop()
  host?.root?.unmount()
  delete global[RENDER_HOST]
}
