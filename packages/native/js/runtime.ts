import { GpuixRenderer } from "@gpuix/native"
import type { EventPayload, WindowOptions } from "@gpuix/native"
import type { App, LiveAutomationRenderer } from "./automation/client.js"
import { createRendererState } from "./renderer-state.js"

const DEFAULT_FRAME_MS = 8
const FRAME_WAKE_FALLBACK_MS = 100
const BROWSER_AUTOMATION = "gpuix"
const PENDING_AUTOMATION = Symbol.for("@gpuix/native/pending-automation")

interface PendingAutomation {
  readonly app: Promise<App>
  cancelled: boolean
}

export interface FrameLoop {
  stop(): void
}

export interface CreateNativeRendererOptions {
  onEvent?: (event: EventPayload) => void
  onError?: (error: unknown) => void
}

function stdinCarriesAutomation(): boolean {
  if (typeof process === "undefined") return false
  try {
    return !process.getBuiltinModule("node:fs").fstatSync(0).isCharacterDevice()
  } catch {
    return false
  }
}

/** Opens the window hidden when the first commit can present it; returns whether to call `presentWindow()`. */
export function initHiddenUntilCommit(renderer: GpuixRenderer, options: WindowOptions): boolean {
  const present = renderer.canPresentWindow?.(options) ?? false
  renderer.init(present ? { ...options, show: false, focus: false } : options)
  return present
}

export function createNativeRenderer(
  options: CreateNativeRendererOptions = {}
): GpuixRenderer {
  let renderer!: GpuixRenderer
  renderer = new GpuixRenderer((error, event) => {
    if (error) {
      options.onError?.(error)
      return
    }
    try {
      if (createRendererState(renderer).dispatch(event)) options.onEvent?.(event)
    } catch (dispatchError) {
      options.onError?.(dispatchError)
    }
  })
  if (stdinCarriesAutomation()) {
    const init = renderer.init.bind(renderer)
    renderer.init = (windowOptions) => {
      init(windowOptions)
      enableAutomation(renderer)
    }
  }
  return renderer
}

export function startFrameLoop(
  renderer: Pick<GpuixRenderer, "requiresTick" | "tick"> &
    Partial<Pick<GpuixRenderer, "setFrameWakeCallback">>,
  options: {
    frameMs?: number
    onTerminated?: () => void
    onError?: (error: unknown) => void
  } = {}
): FrameLoop {
  if (!renderer.requiresTick()) return { stop() {} }
  let timer: ReturnType<typeof setTimeout> | undefined
  let stopped = false
  const stop = () => {
    if (stopped) return
    stopped = true
    if (frameWake) renderer.setFrameWakeCallback?.(null)
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
  }
  const loop = () => {
    if (stopped) return
    if (timer !== undefined) clearTimeout(timer)
    timer = undefined
    const started = performance.now()
    let running = true
    try {
      running = renderer.tick()
    } catch (error) {
      options.onError?.(error)
    }
    if (running === false) {
      stop()
      options.onTerminated?.()
      return
    }
    if (stopped) return
    const delay = frameWake
      ? FRAME_WAKE_FALLBACK_MS
      : Math.max(0, (options.frameMs ?? DEFAULT_FRAME_MS) - (performance.now() - started))
    timer = setTimeout(loop, delay)
  }
  const frameWake = options.frameMs === undefined &&
    renderer.setFrameWakeCallback?.(loop) === true
  loop()
  return { stop }
}

export function enableAutomation(renderer: LiveAutomationRenderer): void {
  void import("./automation/client.js").then(
    ({ InProcessBackend, liveRendererAsTest, serveAutomationStdio }) =>
      serveAutomationStdio(new InProcessBackend(liveRendererAsTest(renderer)))
  )
}

export function installBrowserAutomation(renderer: LiveAutomationRenderer): Promise<App> {
  const published: App | undefined = Reflect.get(globalThis, BROWSER_AUTOMATION)
  if (published) return Promise.resolve(published)
  const installing: PendingAutomation | undefined = Reflect.get(globalThis, PENDING_AUTOMATION)
  if (installing) return installing.app
  const pending: PendingAutomation = {
    cancelled: false,
    app: import("./automation/client.js")
      .then(({ App, browserRendererAsTest, InProcessBackend }) => {
        const app = new App(new InProcessBackend(browserRendererAsTest(renderer)))
        if (pending.cancelled) void app.close()
        else Reflect.set(globalThis, BROWSER_AUTOMATION, app)
        return app
      })
      .finally(() => {
        if (Reflect.get(globalThis, PENDING_AUTOMATION) === pending) {
          Reflect.deleteProperty(globalThis, PENDING_AUTOMATION)
        }
      }),
  }
  Reflect.set(globalThis, PENDING_AUTOMATION, pending)
  return pending.app
}

export function closeBrowserAutomation(): void {
  const pending: PendingAutomation | undefined = Reflect.get(globalThis, PENDING_AUTOMATION)
  if (pending) pending.cancelled = true
  Reflect.deleteProperty(globalThis, PENDING_AUTOMATION)
  const published: App | undefined = Reflect.get(globalThis, BROWSER_AUTOMATION)
  void published?.close()
  Reflect.deleteProperty(globalThis, BROWSER_AUTOMATION)
}
