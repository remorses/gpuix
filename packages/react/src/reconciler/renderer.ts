import React, { type ReactNode } from "react"
import { GpuixRenderer } from "@gpuix/native"
import type { EventPayload, WindowOptions } from "@gpuix/native"
import { createRoot, flushSync, type Root } from "./reconciler.js"
import type {
  DebugFrameOverlayMode,
  NativeRenderer,
  WindowKeyEventHandlers,
} from "../types/host.js"
import { pumpFrames } from "../motion-spring.js"
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

const RUNTIME_ERROR_HANDLERS_KEY = "__gpuixRuntimeErrorHandlers"

type RuntimeErrorHandlers = {
  uncaughtException: (error: Error) => void
  unhandledRejection: (reason: Error | string) => void
}

function runtimeErrorHandlers(): RuntimeErrorHandlers | undefined {
  return Reflect.get(globalThis, RUNTIME_ERROR_HANDLERS_KEY) as
    | RuntimeErrorHandlers
    | undefined
}

/** Keep bun alive after an uncaught throw. A dead process stops AppKit pumps. */
export function installRuntimeErrorHandlers(): void {
  if (typeof process === "undefined" || runtimeErrorHandlers()) return
  const handlers: RuntimeErrorHandlers = {
    uncaughtException: scheduleRuntimeError,
    unhandledRejection: (reason) => {
      scheduleRuntimeError(thrownToError(reason))
    },
  }
  process.on("uncaughtException", handlers.uncaughtException)
  process.on("unhandledRejection", handlers.unhandledRejection)
  Reflect.set(globalThis, RUNTIME_ERROR_HANDLERS_KEY, handlers)
}

function uninstallRuntimeErrorHandlers(): void {
  if (typeof process === "undefined") return
  const handlers = runtimeErrorHandlers()
  if (!handlers) return
  process.off("uncaughtException", handlers.uncaughtException)
  process.off("unhandledRejection", handlers.unhandledRejection)
  Reflect.deleteProperty(globalThis, RUNTIME_ERROR_HANDLERS_KEY)
}

export function createRenderer(
  onEvent?: (event: import("@gpuix/native").EventPayload) => void
): GpuixRenderer {
  const renderer = new GpuixRenderer((err, event) => {
    if (err) {
      console.error("[GPUIX] Native event error:", err)
      return
    }
    try {
      if (handleGpuixEvent(event, renderer) && onEvent) {
        onEvent(event)
      }
    } catch (error) {
      scheduleRuntimeError(thrownToError(error))
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

/** ~125fps. Above any common display refresh rate, so frames are never the
 *  bottleneck, while still leaving the Node event loop almost entirely idle. */
const DEFAULT_FRAME_MS = 8

export interface FrameLoop {
  stop: () => void
}

export function enableAutomation(renderer: LiveAutomationRenderer): void {
  serveAutomationStdio(new InProcessBackend(liveRendererAsTest(renderer)))
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
 * If `tick()` already used the whole budget, wait 0ms. A fixed 8ms sleep after a
 * 10ms frame would cap scroll at ~55fps on a 120Hz display.
 *
 * `tick()` returning false means the last window closed. The loop stops and
 * `onTerminated` runs. `render()` uses that to exit the process.
 *
 * A throw from `tick()` must not stop the timer. On macOS that timer is the
 * AppKit pump; if it dies the window freezes while bun may still be alive.
 */
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

  let lastFrame = performance.now()
  const loop = (): void => {
    if (stopped) return
    const started = performance.now()
    pumpFrames(Math.min((started - lastFrame) / 1000, 0.032), started)
    lastFrame = started
    let running = true
    try {
      running = renderer.tick()
    } catch (error) {
      scheduleRuntimeError(thrownToError(error))
    }
    if (running === false) {
      stop()
      options.onTerminated?.()
      return
    }
    const wait = Math.max(0, frameMs - (performance.now() - started))
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
  lastNode?: ReactNode
  lastOptions?: RenderOptions
  overlayShown?: boolean
}

function formatRuntimeError(
  thrown: Error | string,
  componentStack?: string,
): { message: string; stack: string } {
  let message: string
  let stack: string
  if (thrown instanceof Error) {
    message = thrown.message || thrown.name
    stack = thrown.stack ?? `${thrown.name}: ${thrown.message}`
  } else {
    message = thrown
    stack = thrown
  }
  if (!message) message = "Unknown error"
  const extra = componentStack?.trim()
  if (extra && !stack.includes(extra)) stack = `${stack}\n${extra}`
  if (!stack.includes(message)) stack = `${message}\n${stack}`
  return { message, stack }
}

function thrownToError(thrown: unknown): Error | string {
  if (thrown instanceof Error) return thrown
  if (typeof thrown === "string") return thrown
  try {
    return String(thrown)
  } catch {
    return "Unknown error"
  }
}

const OVERLAY_MONO =
  process.platform === "win32"
    ? "Consolas"
    : process.platform === "darwin"
      ? "Menlo"
      : "DejaVu Sans Mono"

function overlayStackLines(error: { message: string; stack: string }): string[] {
  const lines = error.stack.length === 0 ? [error.message] : error.stack.split("\n")
  const frames = lines.flatMap((line) => {
    const trimmed = line.trim()
    if (trimmed.length === 0) return []
    if (trimmed === error.message) return []
    if (trimmed === `Error: ${error.message}`) return []
    if (/^at\s/.test(line)) return [`    ${line}`]
    return [line]
  })
  return frames.length > 0 ? frames : [error.message]
}

function runtimeErrorOverlay(
  error: { message: string; stack: string },
  onReload: () => void,
): ReactNode {
  const lines = overlayStackLines(error)
  return React.createElement(
    "div",
    {
      testId: "runtime-error-overlay",
      style: {
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
        padding: 32,
        paddingBottom: 40,
        gap: 20,
        backgroundColor: "#000000e6",
        pointerEvents: "auto",
      },
    },
    React.createElement(
      "text",
      {
        style: {
          fontSize: 32,
          fontWeight: 700,
          color: "#e83b46",
          flexShrink: 0,
        },
      },
      "Uncaught runtime errors:",
    ),
    React.createElement(
      "div",
      {
        testId: "runtime-error-stack",
        style: {
          display: "flex",
          flexDirection: "column",
          flexGrow: 0,
          flexShrink: 1,
          minHeight: 0,
          overflowY: "scroll",
          padding: 16,
          paddingBottom: 24,
          backgroundColor: "#ce11261a",
          borderRadius: 4,
          gap: 4,
        },
      },
      React.createElement(
        "text",
        {
          style: {
            fontSize: 20,
            fontWeight: 700,
            color: "#e83b46",
            marginBottom: 12,
            flexShrink: 0,
          },
        },
        error.message,
      ),
      ...lines.map((line) =>
        React.createElement(
          "text",
          {
            style: {
              fontSize: 13,
              lineHeight: 20,
              color: "#fccfcf",
              fontFamily: OVERLAY_MONO,
              whiteSpace: "nowrap",
            },
          },
          line,
        ),
      ),
    ),
    React.createElement(
      "div",
      {
        testId: "runtime-error-reload",
        onClick: onReload,
        style: {
          alignSelf: "flex-start",
          padding: 10,
          paddingLeft: 16,
          paddingRight: 16,
          borderRadius: 4,
          backgroundColor: "#e83b46",
          hover: { backgroundColor: "#c92a34" },
        },
      },
      React.createElement(
        "text",
        { style: { fontSize: 14, fontWeight: 700, color: "#ffffff" } },
        "Reload",
      ),
    ),
  )
}

function mountRoot(args: {
  slot: RenderSlot
  node: ReactNode
  options: RenderOptions
}): Root {
  const { slot, node, options } = args
  const { onEvent, onKeyDown, onKeyUp } = options
  const host = slot.renderer
  if (!host) throw new Error("GPUIX renderer is not initialized")
  if (slot.root) slot.root.unmount()
  const root = createRoot(host, {
    onEvent,
    onKeyDown,
    onKeyUp,
    onUncaughtError: (error, errorInfo) => {
      scheduleRuntimeError(error, errorInfo.componentStack)
    },
  })
  slot.root = root
  slot.overlayShown = false
  flushSync(() => {
    root.render(node)
  })
  return root
}

function reloadApp(slot: RenderSlot): void {
  if (slot.lastNode === undefined) return
  mountRoot({ slot, node: slot.lastNode, options: slot.lastOptions ?? {} })
}

function showRuntimeError(error: Error | string, componentStack?: string): void {
  const formatted = formatRuntimeError(error, componentStack)
  console.error("[gpuix] runtime error:", formatted.stack)
  const slot = Reflect.get(globalThis, RENDER_HOST_KEY) as RenderSlot | undefined
  if (!slot?.root || slot.overlayShown) return
  slot.overlayShown = true
  try {
    mountRoot({
      slot,
      node: runtimeErrorOverlay(formatted, () => reloadApp(slot)),
      options: slot.lastOptions ?? {},
    })
    slot.overlayShown = true
  } catch (overlayError) {
    slot.overlayShown = false
    console.error("[gpuix] failed to show runtime error overlay:", overlayError)
  }
}

function scheduleRuntimeError(error: Error | string, componentStack?: string): void {
  const slot = Reflect.get(globalThis, RENDER_HOST_KEY) as RenderSlot | undefined
  if (!slot?.root) return
  const failedRoot = slot.root
  queueMicrotask(() => {
    const current = Reflect.get(globalThis, RENDER_HOST_KEY) as RenderSlot | undefined
    if (!current?.root || current.root !== failedRoot) return
    showRuntimeError(thrownToError(error), componentStack)
  })
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
  uninstallRuntimeErrorHandlers()
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
      const renderer = createRenderer()
      renderer.init(windowOptions)
      slot.renderer = renderer
      console.log("[gpuix] created native window")
    }
  }
  const host = slot.renderer
  if (!host) {
    throw new Error("GPUIX renderer is not initialized")
  }
  installRuntimeErrorHandlers()
  if (!injected && host instanceof GpuixRenderer && !slot.loop) {
    slot.loop = startFrameLoop(host, {
      onTerminated: () => {
        process.exit(0)
      },
    })
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
  }
  slot.lastNode = node
  slot.lastOptions = { onEvent, onKeyDown, onKeyUp }
  const root = mountRoot({ slot, node, options: slot.lastOptions })
  console.log(remount ? "[gpuix] remount complete" : "[gpuix] mount complete")
  return root
}
