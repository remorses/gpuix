import type { JSX } from "solid-js"
import { GpuixRenderer, type WindowOptions } from "@gpuix/native"
import type { NativeRenderer, RootEventHandlers } from "@gpuix/native/host"
import {
  closeBrowserAutomation,
  createNativeRenderer,
  initHiddenUntilCommit,
  installBrowserAutomation,
  startFrameLoop,
  type FrameLoop,
} from "@gpuix/native/runtime"
import { createRoot, type Root } from "./root.js"

const RENDER_HOST = Symbol.for("@gpuix/solid/render-host")

interface RenderSlot {
  renderer?: NativeRenderer
  root?: Root
  loop?: FrameLoop
}

export interface RenderOptions extends WindowOptions, RootEventHandlers {
  renderer?: NativeRenderer
  debugFrameOverlay?: "hidden" | "minimal" | "full"
}

export function createRenderer(
  onEvent?: (event: import("@gpuix/native").EventPayload) => void
): GpuixRenderer {
  return createNativeRenderer({
    onEvent,
    onError: (error) => console.error("[gpuix-solid] native event error", error),
  })
}

function slot(): RenderSlot {
  const global = globalThis as typeof globalThis & { [RENDER_HOST]?: RenderSlot }
  return global[RENDER_HOST] ??= {}
}

/** Mount Solid. Later calls under `bun --hot` remount on the same window. */
export function render(code: () => JSX.Element, options: RenderOptions = {}): Root {
  const {
    renderer: injected,
    debugFrameOverlay,
    onEvent,
    onKeyDown,
    onKeyUp,
    onSelectionChange,
    tabNavigation,
    keyboardFocusDim,
    onUncaughtError,
    ...windowOptions
  } = options
  const host = slot()
  let presentAfterMount = false
  if (!host.renderer) {
    host.renderer = injected ?? createNativeRenderer({
      onError(error) {
        onUncaughtError?.(
          error instanceof Error ? error : new Error(String(error)),
          {}
        )
      },
    })
    if (!injected && host.renderer instanceof GpuixRenderer) {
      presentAfterMount = initHiddenUntilCommit(host.renderer, windowOptions)
    }
  }
  host.root?.unmount()
  host.root = createRoot(host.renderer, {
    onEvent,
    onKeyDown,
    onKeyUp,
    onSelectionChange,
    tabNavigation,
    keyboardFocusDim,
    onUncaughtError,
  })
  if (typeof window !== "undefined" && host.renderer instanceof GpuixRenderer) {
    installBrowserAutomation(host.renderer).catch((error) =>
      console.error("[gpuix-solid] automation error", error)
    )
  }
  if (debugFrameOverlay) host.renderer.setDebugFrameOverlay?.(debugFrameOverlay)
  try {
    host.root.render(code)
  } finally {
    if (presentAfterMount && host.renderer instanceof GpuixRenderer) host.renderer.presentWindow()
    if (!injected && host.renderer instanceof GpuixRenderer && !host.loop) {
      host.loop = startFrameLoop(host.renderer, {
        onTerminated: () => {
          if (typeof process !== "undefined") process.exit(0)
        },
        onError: (error) => console.error("[gpuix-solid] frame error", error),
      })
    }
  }
  return host.root
}

export function resetRender(): void {
  const global = globalThis as typeof globalThis & { [RENDER_HOST]?: RenderSlot }
  const host = global[RENDER_HOST]
  host?.loop?.stop()
  host?.root?.unmount()
  closeBrowserAutomation()
  delete global[RENDER_HOST]
}
