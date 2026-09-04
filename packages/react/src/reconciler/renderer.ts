import type { ReactNode } from "react"
import { GpuixRenderer } from "@gpuix/native"
import type { EventPayload, WindowOptions } from "@gpuix/native"
import { createRoot, flushSync, type Root } from "./reconciler.js"
import type {
  DebugFrameOverlayMode,
  NativeRenderer,
  WindowKeyEventHandlers,
} from "../types/host.js"
import { handleGpuixEvent } from "./event-registry.js"
import {
  App as AutomationApp,
  browserRendererAsTest,
  InProcessBackend,
  liveRendererAsTest,
  serveAutomationStdio,
  type LiveAutomationRenderer,
} from "../automation/client.js"

export { createRoot, flushSync, reconciler } from "./reconciler.js"
export type { Root } from "./reconciler.js"

export function createRenderer(
  onEvent?: (event: import("@gpuix/native").EventPayload) => void
): GpuixRenderer {
  const renderer = new GpuixRenderer((err, event) => {
    if (err) {
      console.error("[GPUIX] Native event error:", err)
      return
    }
    handleGpuixEvent(event, renderer)
    if (onEvent) {
      onEvent(event)
    }
  })
  // A pipe means a controller owns stdin. A TTY is a human keyboard.
  if (typeof process !== "undefined" && process.stdin && !process.stdin.isTTY) {
    const init = renderer.init.bind(renderer)
    renderer.init = (options) => {
      init(options)
      enableAutomation(renderer)
    }
  }
  return renderer
}

/** ~125Hz keeps native input polling above common display rates. The embedded
 * AppKit pump is nonblocking, so the interval remains JavaScript yield time. */
const DEFAULT_FRAME_MS = 8
const MIN_FRAME_YIELD_MS = 4

export interface FrameLoop {
  stop: () => void
}

/**
 * Drive GPUI until the last window closes, then run `onTerminated`.
 *
 * On macOS, `renderer.tick()` pumps AppKit and asks GPUI for a frame, so it
 * must be called repeatedly. Do NOT call it from a `setImmediate` loop: that
 * spins the CPU at tens of thousands of ticks per second (measured: 73% CPU on
 * an idle app, versus 1.5% when paced).
 *
 * On Windows and Linux, GPUI owns a blocking event loop on a Rust UI thread.
 * `tick()` does not pump that loop. It only reports whether the UI thread is
 * still inside `Platform::run`. The timer still exists so last-window-close
 * can return false and `render()` can `process.exit`, matching macOS.
 *
 * Pacing lives in JS rather than blocking inside `tick()` on purpose. Node owns
 * the event loop here, so a blocking tick would stall every timer, promise and
 * socket in the process.
 *
 * Each frame is scheduled only after the previous one finishes, so a slow frame
 * delays the next one instead of letting timers pile up.
 *
 * `tick()` normally returns immediately. If an exceptional tick uses the whole
 * budget, still yield briefly so PTY, timer, promise, and socket callbacks cannot
 * be monopolized by consecutive native pumps.
 *
 * `tick()` returning false means the last window closed. The loop stops and
 * `onTerminated` runs. `render()` uses that to exit the process.
 */
export function enableAutomation(renderer: LiveAutomationRenderer): void {
  serveAutomationStdio(new InProcessBackend(liveRendererAsTest(renderer)))
}

export function startFrameLoop(
  renderer: Pick<GpuixRenderer, "requiresTick" | "tick">,
  options: { frameMs?: number; onTerminated?: () => void } = {}
): FrameLoop {
  if (!renderer.requiresTick()) {
    return { stop: () => {} }
  }

  const frameMs = options.frameMs ?? DEFAULT_FRAME_MS
  let timer: ReturnType<typeof setTimeout> | null = null
  let stopped = false

  const stop = (): void => {
    stopped = true
    if (timer !== null) clearTimeout(timer)
    timer = null
  }

  const loop = (): void => {
    if (stopped) return
    const started = performance.now()
    const running = renderer.tick()
    if (running === false) {
      stop()
      options.onTerminated?.()
      return
    }
    const wait = Math.max(MIN_FRAME_YIELD_MS, frameMs - (performance.now() - started))
    timer = setTimeout(loop, wait)
  }
  loop()

  return { stop }
}

const RENDER_HOST_KEY = "__gpuixRenderHost"
const BROWSER_AUTOMATION_KEY = "gpuix"

declare global {
  var gpuix: AutomationApp | undefined
}

export function installBrowserAutomation(
  renderer: LiveAutomationRenderer
): AutomationApp {
  const existing = Reflect.get(globalThis, BROWSER_AUTOMATION_KEY)
  if (existing instanceof AutomationApp) return existing

  const automation = new AutomationApp(
    new InProcessBackend(browserRendererAsTest(renderer))
  )
  Reflect.set(globalThis, BROWSER_AUTOMATION_KEY, automation)
  return automation
}

type RenderSlot = {
  renderer?: NativeRenderer
  root?: Root
  loop?: FrameLoop
}

function renderSlot(): RenderSlot {
  const existing = Reflect.get(globalThis, RENDER_HOST_KEY)
  if (existing) {
    return existing
  }
  const created: RenderSlot = {}
  Reflect.set(globalThis, RENDER_HOST_KEY, created)
  return created
}

export interface RenderOptions extends WindowOptions, WindowKeyEventHandlers {
  onEvent?: (event: EventPayload) => void
  renderer?: NativeRenderer
  /** GPUI scene overlay. Does not go through React or layout. */
  debugFrameOverlay?: DebugFrameOverlayMode
}

export function resetRender(): void {
  const slot = Reflect.get(globalThis, RENDER_HOST_KEY) as RenderSlot | undefined
  slot?.loop?.stop()
  slot?.root?.unmount()
  const automation = Reflect.get(globalThis, BROWSER_AUTOMATION_KEY)
  void automation?.close()
  Reflect.deleteProperty(globalThis, BROWSER_AUTOMATION_KEY)
  Reflect.deleteProperty(globalThis, RENDER_HOST_KEY)
}

/** Mount the app. Under `bun --hot`, later calls remount on the same native window. */
export function render(node: ReactNode, options: RenderOptions = {}): Root {
  const {
    onEvent,
    onKeyDown,
    onKeyUp,
    renderer: injected,
    debugFrameOverlay,
    ...windowOptions
  } = options
  const slot = renderSlot()
  const remount = slot.root != null
  if (!slot.renderer) {
    if (injected) {
      slot.renderer = injected
    } else {
      const renderer = createRenderer(onEvent)
      renderer.init(windowOptions)
      slot.renderer = renderer
      console.log("[gpuix] created native window")
    }
  }
  const host = slot.renderer
  if (!host) {
    throw new Error("GPUIX renderer is not initialized")
  }
  if (
    typeof window !== "undefined" &&
    host instanceof GpuixRenderer &&
    !Reflect.has(globalThis, BROWSER_AUTOMATION_KEY)
  ) {
    installBrowserAutomation(host)
  }
  if (debugFrameOverlay) {
    host.setDebugFrameOverlay?.(debugFrameOverlay)
  }
  if (slot.root) {
    console.log("[gpuix] remount: unmount previous tree")
    slot.root.unmount()
  }
  const root = createRoot(host, { onKeyDown, onKeyUp })
  slot.root = root
  flushSync(() => {
    root.render(node)
  })
  if (!injected && slot.renderer instanceof GpuixRenderer) {
    const native = slot.renderer
    slot.loop?.stop()
    slot.loop = startFrameLoop(native, {
      onTerminated: () => {
        process.exit(0)
      },
    })
  }
  console.log(remount ? "[gpuix] remount complete" : "[gpuix] mount complete")
  return root
}
