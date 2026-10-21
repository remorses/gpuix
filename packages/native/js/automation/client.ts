/// Playwright-like automation client for GPUIX.
///
/// In-process tests talk to TestRenderer through the same typed method catalog
/// as a live app on SSE stdin/stdout. Locators query the retained tree.

import {
  AutomationError,
  createSseDecoder,
  encodeSse,
  methods,
  parseResponse,
  parseWireMessage,
  PROTOCOL_VERSION,
  type AutomationRequest,
  type AutomationResponse,
  type AutomationServerEvent,
  type ElementBounds,
  type MethodName,
  type ParamsOf,
  type ResultOf,
  type TreeNode,
} from "./protocol.js"

function importNodeModule<T>(specifier: string): Promise<T> {
  return import(specifier)
}

export interface AutomationBackend {
  call<M extends MethodName>(method: M, params: ParamsOf<M>): Promise<ResultOf<M>>
  close(): Promise<void>
}

abstract class ValidatedAutomationBackend implements AutomationBackend {
  private closed = false

  async call<M extends MethodName>(
    method: M,
    params: ParamsOf<M>
  ): Promise<ResultOf<M>> {
    if (this.closed) {
      throw new AutomationError("Closed", "Automation session is closed")
    }
    const parsedParams = methods[method].params.parse(params) as ParamsOf<M>
    const result = await this.request(method, parsedParams)
    return methods[method].result.parse(result) as ResultOf<M>
  }

  protected closeSession(): boolean {
    if (this.closed) return false
    this.closed = true
    return true
  }

  protected abstract request<M extends MethodName>(
    method: M,
    params: ParamsOf<M>
  ): unknown | Promise<unknown>

  abstract close(): Promise<void>
}

export interface TestAutomationRenderer {
  nativeSimulateClick(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void
  nativeSimulateMouseDown(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void
  nativeSimulateMouseUp(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void
  nativeSimulateMouseMove(
    x: number,
    y: number,
    pressedButton?: number,
    modifiers?: string
  ): void
  nativeSimulateScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
    modifiers?: string
  ): void
  simulateKeystrokes(keystrokes: string): void
  nativeSimulateKeystrokes(elementId: number, keystrokes: string): void
  nativeSimulateKeyDown(
    elementId: number,
    keystroke: string,
    isHeld?: boolean
  ): void
  nativeSimulateKeyUp(elementId: number, keystroke: string): void
  scrollTo(elementId: number, x: number, y: number): void
  getScrollOffset(elementId: number): [number, number] | null
  getAllText(): string[]
  getPaintedText(): string[]
  getSelectedText(): string | null
  clearSelection(): void
  captureScreenshot(path: string): void
  getAutomationTree(): string
  getElementBounds(elementId: number): { x: number; y: number; width: number; height: number } | null
  clockPause(): number
  clockSet(nowMs: number): number
  clockFastForward(deltaMs: number): number
  clockResume(): number
  focusElement?(elementId: number): void
  blur?(): void
}

type HandlerMap = {
  [M in MethodName]: (params: ParamsOf<M>) => ResultOf<M> | Promise<ResultOf<M>>
}

export class InProcessBackend extends ValidatedAutomationBackend {
  constructor(private readonly renderer: TestAutomationRenderer) {
    super()
  }

  protected request<M extends MethodName>(
    method: M,
    params: ParamsOf<M>
  ): unknown | Promise<unknown> {
    return this.handlers[method](params as never)
  }

  async close(): Promise<void> {
    this.closeSession()
  }

  private readonly handlers: HandlerMap = {
    initialize: () => ({
      protocolVersion: PROTOCOL_VERSION,
      pid: typeof process === "undefined" ? 0 : process.pid,
      capabilities: typeof window !== "undefined"
        ? ["input", "clock", "tree"]
        : ["input", "screenshot", "clock", "tree"],
      window: (() => {
        return {
          width: typeof window === "undefined" ? 800 : window.innerWidth,
          height: typeof window === "undefined" ? 600 : window.innerHeight,
        }
      })(),
    }),
    cancel: () => ({ ok: true as const }),
    click: (params) => {
      this.renderer.nativeSimulateClick(
        params.x,
        params.y,
        params.button,
        params.modifiers
      )
      return { ok: true as const }
    },
    mouseDown: (params) => {
      this.renderer.nativeSimulateMouseDown(
        params.x,
        params.y,
        params.button,
        params.modifiers
      )
      return { ok: true as const }
    },
    mouseUp: (params) => {
      this.renderer.nativeSimulateMouseUp(
        params.x,
        params.y,
        params.button,
        params.modifiers
      )
      return { ok: true as const }
    },
    mouseMove: (params) => {
      this.renderer.nativeSimulateMouseMove(
        params.x,
        params.y,
        params.pressedButton,
        params.modifiers
      )
      return { ok: true as const }
    },
    scrollWheel: (params) => {
      this.renderer.nativeSimulateScrollWheel(
        params.x,
        params.y,
        params.deltaX,
        params.deltaY,
        params.modifiers
      )
      return { ok: true as const }
    },
    keystrokes: (params) => {
      if (params.elementId == null) {
        this.renderer.simulateKeystrokes(params.keys)
      } else {
        this.renderer.nativeSimulateKeystrokes(params.elementId, params.keys)
      }
      return { ok: true as const }
    },
    keyDown: (params) => {
      this.renderer.nativeSimulateKeyDown(
        params.elementId ?? 0,
        params.key,
        params.isHeld
      )
      return { ok: true as const }
    },
    keyUp: (params) => {
      this.renderer.nativeSimulateKeyUp(params.elementId ?? 0, params.key)
      return { ok: true as const }
    },
    focus: (params) => {
      this.renderer.focusElement?.(params.elementId)
      return { ok: true as const }
    },
    blur: () => {
      this.renderer.blur?.()
      return { ok: true as const }
    },
    scrollTo: (params) => {
      this.renderer.scrollTo(params.elementId, params.x, params.y)
      return { ok: true as const }
    },
    getScrollOffset: (params) => ({
      offset: this.renderer.getScrollOffset(params.elementId),
    }),
    getTree: () => {
      const raw = JSON.parse(this.renderer.getAutomationTree()) as unknown
      return { tree: raw === null ? null : (raw as TreeNode) }
    },
    getPaintedText: () => ({ text: this.renderer.getPaintedText() }),
    getAllText: () => ({ text: this.renderer.getAllText() }),
    getBounds: (params) => {
      const rect = this.renderer.getElementBounds(params.elementId)
      if (!rect) return { bounds: null }
      return { bounds: rect }
    },
    getSelectedText: () => ({ text: this.renderer.getSelectedText() }),
    clearSelection: () => {
      this.renderer.clearSelection()
      return { ok: true as const }
    },
    screenshot: (params) => {
      this.renderer.captureScreenshot(params.path)
      return { path: params.path }
    },
    clockPause: () => ({ nowMs: this.renderer.clockPause() }),
    clockSet: (params) => ({ nowMs: this.renderer.clockSet(params.nowMs) }),
    clockFastForward: (params) => ({
      nowMs: this.renderer.clockFastForward(params.deltaMs),
    }),
    clockResume: () => ({ nowMs: this.renderer.clockResume() }),
  }
}

class PendingAutomationResponse {
  readonly promise: Promise<AutomationResponse>
  readonly resolve: (response: AutomationResponse) => void
  readonly reject: (error: unknown) => void

  constructor() {
    let resolveResponse!: (response: AutomationResponse) => void
    let rejectResponse!: (error: unknown) => void
    this.promise = new Promise<AutomationResponse>((resolve, reject) => {
      resolveResponse = resolve
      rejectResponse = reject
    })
    this.resolve = resolveResponse
    this.reject = rejectResponse
  }
}

export class SseBackend extends ValidatedAutomationBackend {
  private nextId = 1
  private readonly pending = new Map<number, PendingAutomationResponse>()

  constructor(
    private readonly write: (chunk: string) => void,
    feed: (listener: (chunk: string) => void) => void,
    private readonly onClose?: () => Promise<void>
  ) {
    super()
    const decoder = createSseDecoder((message) => {
      if ("method" in message) return
      if ("event" in message) return
      const waiter = this.pending.get(message.id)
      if (!waiter) return
      this.pending.delete(message.id)
      waiter.resolve(message)
    })
    feed((chunk) => decoder.feed(chunk))
  }

  protected async request<M extends MethodName>(
    method: M,
    params: ParamsOf<M>
  ): Promise<unknown> {
    const id = this.nextId++
    const request = { id, method, params } as AutomationRequest
    const pending = new PendingAutomationResponse()
    this.pending.set(id, pending)
    try {
      this.write(encodeSse(request))
    } catch (error) {
      this.pending.delete(id)
      pending.reject(error)
    }

    const response = await pending.promise
    if ("error" in response) {
      throw new AutomationError(
        response.error.code,
        response.error.message,
        response.error.data
      )
    }
    return response.result
  }

  async close(): Promise<void> {
    if (!this.closeSession()) return
    for (const [id, waiter] of this.pending) {
      waiter.reject(new AutomationError("Closed", `Request ${id} cancelled`))
    }
    this.pending.clear()
    await this.onClose?.()
  }
}

function toKeystrokes(text: string): string {
  return [...text]
    .map((ch) => {
      if (ch === " ") return "space"
      if (ch === "\n") return "enter"
      if (ch === "\t") return "tab"
      return ch
    })
    .join(" ")
}

interface Selector {
  testId?: string
  text?: string
  type?: string
  parent?: Selector
}

function matches(node: TreeNode, selector: Selector): boolean {
  if (selector.testId != null && node.testId !== selector.testId) return false
  if (selector.type != null && node.type !== selector.type) return false
  if (selector.text != null && !(node.text ?? "").includes(selector.text)) {
    return false
  }
  return true
}

function collect(node: TreeNode | null, selector: Selector): TreeNode[] {
  if (!node) return []
  const roots = selector.parent
    ? collect(node, selector.parent)
    : [node]
  const found: TreeNode[] = []
  const visited = new Set<number>()
  const walk = (current: TreeNode) => {
    if (visited.has(current.id)) return
    visited.add(current.id)
    if (matches(current, selector)) found.push(current)
    for (const child of current.children ?? []) walk(child)
  }
  for (const root of roots) {
    if (selector.parent) {
      for (const child of root.children ?? []) walk(child)
    } else {
      walk(root)
    }
  }
  return found
}

/** A window-space point, or a locator resolved to the centre of its bounds. */
export type PointTarget = { x: number; y: number } | Locator

export interface MouseOptions {
  /** 0 = left (default), 1 = middle, 2 = right. */
  button?: number
  /** Held modifiers in `press()` syntax: `"cmd"`, `"cmd-shift"`, `"alt"`. */
  modifiers?: string
}

export interface DragOptions extends MouseOptions {
  /**
   * Move events sent between the press and the release. Default 8.
   *
   * One jump would test nothing that matters: snapping, live previews and
   * per-move commits only appear when the pointer actually travels.
   */
  steps?: number
  /** Pixels from the source centre where the press lands. */
  offset?: { x: number; y: number }
}

function centerOf(bounds: ElementBounds): { x: number; y: number } {
  return { x: bounds.x + bounds.width / 2, y: bounds.y + bounds.height / 2 }
}

function collectText(node: TreeNode): string {
  let text = node.text ?? ""
  for (const child of node.children ?? []) text += collectText(child)
  return text
}

export class Locator {
  constructor(
    private readonly app: App,
    private readonly selector: Selector
  ) {}

  getByTestId(testId: string): Locator {
    return new Locator(this.app, { testId, parent: this.selector })
  }

  getByText(text: string): Locator {
    return new Locator(this.app, { text, parent: this.selector })
  }

  getByType(type: string): Locator {
    return new Locator(this.app, { type, parent: this.selector })
  }

  async all(): Promise<TreeNode[]> {
    const { tree } = await this.app.call("getTree", {})
    return collect(tree, this.selector)
  }

  async count(): Promise<number> {
    return (await this.all()).length
  }

  async element(): Promise<TreeNode> {
    const found = await this.all()
    if (found.length === 0) {
      throw new AutomationError("NotFound", "Locator did not match any element")
    }
    if (found.length > 1) {
      throw new AutomationError(
        "Ambiguous",
        `Locator matched ${found.length} elements`
      )
    }
    return found[0]
  }

  async bounds(): Promise<ElementBounds> {
    const node = await this.element()
    if (node.bounds) return node.bounds
    const { bounds } = await this.app.call("getBounds", { elementId: node.id })
    if (!bounds) {
      throw new AutomationError("NotFound", "Element has no painted bounds")
    }
    return bounds
  }

  /** The centre of the last painted bounds, in window coordinates. */
  async center(): Promise<{ x: number; y: number }> {
    return centerOf(await this.bounds())
  }

  async click(options: MouseOptions = {}): Promise<void> {
    const point = await this.center()
    await this.app.call("click", { ...point, ...options })
  }

  /** Move the pointer to the centre, so hover styles and tooltips fire. */
  async hover(options: MouseOptions = {}): Promise<void> {
    await this.app.mouse.move(await this.center(), options)
  }

  /** Send one wheel event over the centre of this element. */
  async wheel(
    deltaX: number,
    deltaY: number,
    options: MouseOptions = {}
  ): Promise<void> {
    await this.app.mouse.wheel(await this.center(), deltaX, deltaY, options)
  }

  /** Press on this element, travel to `target`, release there. */
  async dragTo(target: PointTarget, options: DragOptions = {}): Promise<void> {
    await this.app.mouse.drag(this, target, options)
  }

  /** Press on this element and release `dx`/`dy` pixels away. */
  async dragBy(dx: number, dy: number, options: DragOptions = {}): Promise<void> {
    const start = await this.center()
    const offset = options.offset ?? { x: 0, y: 0 }
    await this.app.mouse.drag(
      this,
      { x: start.x + offset.x + dx, y: start.y + offset.y + dy },
      options
    )
  }

  async fill(text: string): Promise<void> {
    const node = await this.element()
    const browserPlatform = typeof navigator === "undefined" ? "" : navigator.platform
    const selectAll =
      browserPlatform.includes("Mac") ||
      (typeof process !== "undefined" && process.platform === "darwin")
        ? "cmd-a"
        : "ctrl-a"
    const replacement = text.length === 0 ? "backspace" : toKeystrokes(text)
    await this.app.call("keystrokes", {
      elementId: node.id,
      keys: `${selectAll} ${replacement}`,
    })
  }

  async press(key: string): Promise<void> {
    const node = await this.element()
    await this.app.call("keystrokes", {
      elementId: node.id,
      keys: key,
    })
  }

  /**
   * Own text plus every descendant's, concatenated in document order, like
   * DOM `textContent`. `<text>{value}</text>` puts the string on a child node,
   * so reading only `node.text` returned an empty string for every wrapper.
   */
  async textContent(): Promise<string> {
    return collectText(await this.element())
  }

  async waitFor(options: { timeoutMs?: number } = {}): Promise<TreeNode> {
    const timeoutMs = options.timeoutMs ?? 5000
    const started = Date.now()
    for (;;) {
      const found = await this.all()
      if (found.length === 1) return found[0]
      if (Date.now() - started >= timeoutMs) {
        throw new AutomationError(
          found.length === 0 ? "Timeout" : "Ambiguous",
          `waitFor timed out after ${timeoutMs}ms`
        )
      }
      await new Promise((resolve) => setTimeout(resolve, 16))
    }
  }
}

export class App {
  readonly clock: {
    pause: () => Promise<number>
    set: (nowMs: number) => Promise<number>
    fastForward: (deltaMs: number) => Promise<number>
    resume: () => Promise<number>
  }

  /**
   * Raw pointer input in window coordinates. Prefer a locator when the target
   * is an element; use this for empty space, marquee selection, and gestures
   * that end outside every hitbox.
   */
  readonly mouse: {
    move: (
      target: PointTarget,
      options?: MouseOptions & { pressedButton?: number }
    ) => Promise<void>
    down: (target: PointTarget, options?: MouseOptions) => Promise<void>
    up: (target: PointTarget, options?: MouseOptions) => Promise<void>
    click: (target: PointTarget, options?: MouseOptions) => Promise<void>
    wheel: (
      target: PointTarget,
      deltaX: number,
      deltaY: number,
      options?: MouseOptions
    ) => Promise<void>
    drag: (
      from: PointTarget,
      to: PointTarget,
      options?: DragOptions
    ) => Promise<void>
  }

  constructor(private readonly backend: AutomationBackend) {
    this.mouse = {
      move: async (target, options = {}) => {
        const point = await this.resolvePoint(target)
        await this.call("mouseMove", {
          ...point,
          pressedButton: options.pressedButton,
          modifiers: options.modifiers,
        })
      },
      down: async (target, options = {}) => {
        const point = await this.resolvePoint(target)
        await this.call("mouseDown", { ...point, ...options })
      },
      up: async (target, options = {}) => {
        const point = await this.resolvePoint(target)
        await this.call("mouseUp", { ...point, ...options })
      },
      click: async (target, options = {}) => {
        const point = await this.resolvePoint(target)
        await this.call("click", { ...point, ...options })
      },
      wheel: async (target, deltaX, deltaY, options = {}) => {
        const point = await this.resolvePoint(target)
        await this.call("scrollWheel", {
          ...point,
          deltaX,
          deltaY,
          modifiers: options.modifiers,
        })
      },
      drag: async (from, to, options = {}) => {
        const offset = options.offset ?? { x: 0, y: 0 }
        const origin = await this.resolvePoint(from)
        const start = { x: origin.x + offset.x, y: origin.y + offset.y }
        // Resolve the destination before the press. A locator target can move
        // under a live preview, and the caller means where it is now.
        const end = await this.resolvePoint(to)
        const button = options.button ?? 0
        const modifiers = options.modifiers
        const steps = Math.max(1, Math.floor(options.steps ?? 8))

        await this.call("mouseMove", { ...start, modifiers })
        await this.call("mouseDown", { ...start, button, modifiers })
        for (let step = 1; step <= steps; step += 1) {
          const t = step / steps
          await this.call("mouseMove", {
            x: start.x + (end.x - start.x) * t,
            y: start.y + (end.y - start.y) * t,
            pressedButton: button,
            modifiers,
          })
        }
        await this.call("mouseUp", { ...end, button, modifiers })
      },
    }
    this.clock = {
      pause: async () => (await this.call("clockPause", {})).nowMs,
      set: async (nowMs) => (await this.call("clockSet", { nowMs })).nowMs,
      fastForward: async (deltaMs) =>
        (await this.call("clockFastForward", { deltaMs })).nowMs,
      resume: async () => (await this.call("clockResume", {})).nowMs,
    }
  }

  call<M extends MethodName>(
    method: M,
    params: ParamsOf<M>
  ): Promise<ResultOf<M>> {
    return this.backend.call(method, params)
  }

  private async resolvePoint(target: PointTarget): Promise<{ x: number; y: number }> {
    return target instanceof Locator ? target.center() : target
  }

  getByTestId(testId: string): Locator {
    return new Locator(this, { testId })
  }

  getByText(text: string): Locator {
    return new Locator(this, { text })
  }

  getByType(type: string): Locator {
    return new Locator(this, { type })
  }

  async screenshot(options: { path: string }): Promise<string> {
    const { path: saved } = await this.call("screenshot", { path: options.path })
    return saved
  }

  async captureFrames(
    dir: string,
    timesMs: readonly number[]
  ): Promise<string[]> {
    if (typeof process === "undefined") {
      throw new AutomationError(
        "Unsupported",
        "Browser frame capture must use the controlling browser automation client"
      )
    }
    const { mkdir } = await importNodeModule<typeof import("node:fs/promises")>(
      "node:fs/promises"
    )
    const path = await importNodeModule<typeof import("node:path")>("node:path")
    await mkdir(dir, { recursive: true })
    await this.clock.pause()
    const paths: string[] = []
    for (const nowMs of timesMs) {
      await this.clock.set(nowMs)
      const file = path.join(dir, `t${nowMs}.png`)
      await this.screenshot({ path: file })
      paths.push(file)
    }
    return paths
  }

  /**
   * Record a video on the paused clock. Every frame is a screenshot taken at
   * an exact timestamp, so the result is smooth at any `fps` no matter how
   * long a frame takes to render. Input between frames lands instantly.
   */
  async recordVideo(options: { path: string; fps?: number }): Promise<VideoRecording> {
    if (typeof process === "undefined") {
      throw new AutomationError(
        "Unsupported",
        "Browser video capture must use the controlling browser automation client"
      )
    }
    const fps = options.fps ?? 60
    if (!(fps > 0)) {
      throw new AutomationError("Protocol", `fps must be positive, got ${fps}`)
    }
    const fs = await importNodeModule<typeof import("node:fs/promises")>("node:fs/promises")
    const os = await importNodeModule<typeof import("node:os")>("node:os")
    const path = await importNodeModule<typeof import("node:path")>("node:path")
    const framesDir = await fs.mkdtemp(path.join(os.tmpdir(), "gpuix-video-"))
    const startMs = await this.clock.pause()
    const recording = new VideoRecording({
      app: this,
      path: options.path,
      fps,
      startMs,
      framesDir,
    })
    await recording.frame()
    return recording
  }

  async close(): Promise<void> {
    await this.backend.close()
  }
}

/** A video in progress. Created by `app.recordVideo()`. */
export class VideoRecording {
  readonly path: string
  readonly fps: number
  private frames = 0
  private stopped = false
  private readonly app: App
  private readonly startMs: number
  private readonly framesDir: string

  constructor(options: {
    app: App
    path: string
    fps: number
    startMs: number
    framesDir: string
  }) {
    this.app = options.app
    this.path = options.path
    this.fps = options.fps
    this.startMs = options.startMs
    this.framesDir = options.framesDir
  }

  /** Clock time of the next frame, in ms. */
  get nowMs(): number {
    return this.startMs + (this.frames * 1000) / this.fps
  }

  /** Set the clock to the next frame time and capture it. */
  async frame(): Promise<void> {
    if (this.stopped) {
      throw new AutomationError("Closed", "Video recording already stopped")
    }
    await this.app.clock.set(this.nowMs)
    const name = `frame-${String(this.frames).padStart(6, "0")}.png`
    await this.app.screenshot({ path: `${this.framesDir}/${name}` })
    this.frames += 1
  }

  /** Record `ms` of video, one frame per `1000 / fps` ms of clock time. */
  async wait(ms: number): Promise<void> {
    const count = Math.round((ms * this.fps) / 1000)
    for (let index = 0; index < count; index += 1) await this.frame()
  }

  /** Encode the frames with ffmpeg, delete them, and resume the clock. */
  async stop(): Promise<string> {
    if (this.stopped) return this.path
    this.stopped = true
    const fs = await importNodeModule<typeof import("node:fs/promises")>("node:fs/promises")
    const nodePath = await importNodeModule<typeof import("node:path")>("node:path")
    const { spawn } = await importNodeModule<typeof import("node:child_process")>(
      "node:child_process"
    )
    try {
      await fs.mkdir(nodePath.dirname(nodePath.resolve(this.path)), { recursive: true })
      const extension = nodePath.extname(this.path).toLowerCase()
      // H.264 needs even dimensions and 4:2:0 to play in browsers and QuickTime.
      const codec =
        extension === ".webm"
          ? ["-c:v", "libvpx-vp9", "-b:v", "0", "-crf", "20"]
          : extension === ".gif"
            ? []
            : ["-c:v", "libx264", "-crf", "14", "-pix_fmt", "yuv420p",
               "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2"]
      const args = [
        "-y", "-v", "error",
        "-framerate", String(this.fps),
        "-i", nodePath.join(this.framesDir, "frame-%06d.png"),
        ...codec,
        this.path,
      ]
      await new Promise<void>((resolve, reject) => {
        const child = spawn("ffmpeg", args, { stdio: ["ignore", "ignore", "pipe"] })
        let stderr = ""
        child.stderr.on("data", (chunk: Buffer) => {
          stderr += chunk.toString("utf8")
        })
        child.on("error", (error) => {
          reject(
            new AutomationError(
              "Unsupported",
              `recordVideo needs ffmpeg on PATH: ${error.message}`
            )
          )
        })
        child.on("close", (code) => {
          if (code === 0) resolve()
          else reject(new AutomationError("Protocol", `ffmpeg exited with ${code}: ${stderr}`))
        })
      })
    } finally {
      await fs.rm(this.framesDir, { recursive: true, force: true })
      await this.app.clock.resume()
    }
    return this.path
  }
}

export interface LiveAutomationRenderer {
  simulateClick(x: number, y: number, button?: number, modifiers?: string): void
  simulateMouseDown(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void
  simulateMouseUp(x: number, y: number, button?: number, modifiers?: string): void
  simulateMouseMove(
    x: number,
    y: number,
    pressedButton?: number,
    modifiers?: string
  ): void
  simulateScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
    modifiers?: string
  ): void
  simulateKeystrokes?(keystrokes: string): void
  simulateKeyDown?(keystroke: string, isHeld?: boolean): void
  simulateKeyUp?(keystroke: string): void
  tick?(): void
  focusElement(elementId: number): void
  blur(): void
  scrollTo(elementId: number, x: number, y: number): void
  getScrollOffset(elementId: number): number[] | null
  getAllText(): string[]
  getPaintedText(): string[]
  getSelectedText(): string | null
  clearSelection(): void
  captureScreenshot?(path: string): void
  getAutomationTree(): string
  getElementBounds(elementId: number): { x: number; y: number; width: number; height: number } | null
  clockPause(): number
  clockSet(nowMs: number): number
  clockFastForward(deltaMs: number): number
  clockResume(): number
}

export function liveRendererAsTest(
  renderer: LiveAutomationRenderer
): TestAutomationRenderer {
  const afterInput = (): void => {
    renderer.tick?.()
  }
  return {
    nativeSimulateClick(x, y, button, modifiers) {
      renderer.simulateClick(x, y, button, modifiers)
      afterInput()
    },
    nativeSimulateMouseDown(x, y, button, modifiers) {
      renderer.simulateMouseDown(x, y, button, modifiers)
      afterInput()
    },
    nativeSimulateMouseUp(x, y, button, modifiers) {
      renderer.simulateMouseUp(x, y, button, modifiers)
      afterInput()
    },
    nativeSimulateMouseMove(x, y, pressedButton, modifiers) {
      renderer.simulateMouseMove(x, y, pressedButton, modifiers)
      afterInput()
    },
    nativeSimulateScrollWheel(x, y, deltaX, deltaY, modifiers) {
      renderer.simulateScrollWheel(x, y, deltaX, deltaY, modifiers)
      afterInput()
    },
    simulateKeystrokes(keys) {
      if (!renderer.simulateKeystrokes) {
        throw new AutomationError("Unsupported", "keystrokes are not live yet")
      }
      renderer.simulateKeystrokes(keys)
      afterInput()
    },
    nativeSimulateKeystrokes(elementId, keys) {
      if (!renderer.simulateKeystrokes) {
        throw new AutomationError("Unsupported", "keystrokes are not live yet")
      }
      renderer.focusElement(elementId)
      renderer.simulateKeystrokes(keys)
      afterInput()
    },
    nativeSimulateKeyDown(elementId, key, isHeld) {
      if (!renderer.simulateKeyDown) {
        throw new AutomationError("Unsupported", "keyDown is not live yet")
      }
      if (elementId > 0) renderer.focusElement(elementId)
      renderer.simulateKeyDown(key, isHeld)
      afterInput()
    },
    nativeSimulateKeyUp(elementId, key) {
      if (!renderer.simulateKeyUp) {
        throw new AutomationError("Unsupported", "keyUp is not live yet")
      }
      if (elementId > 0) renderer.focusElement(elementId)
      renderer.simulateKeyUp(key)
      afterInput()
    },
    scrollTo: (id, x, y) => renderer.scrollTo(id, x, y),
    getScrollOffset: (id) => {
      const offset = renderer.getScrollOffset(id)
      return offset ? [offset[0], offset[1]] : null
    },
    getAllText: () => renderer.getAllText(),
    getPaintedText: () => renderer.getPaintedText(),
    getSelectedText: () => renderer.getSelectedText(),
    clearSelection: () => renderer.clearSelection(),
    captureScreenshot(file) {
      if (!renderer.captureScreenshot) {
        throw new AutomationError(
          "Unsupported",
          "Browser screenshots must use the controlling browser automation client"
        )
      }
      renderer.captureScreenshot(file)
    },
    getAutomationTree: () => renderer.getAutomationTree(),
    getElementBounds: (id) => renderer.getElementBounds(id),
    clockPause: () => renderer.clockPause(),
    clockSet: (nowMs) => renderer.clockSet(nowMs),
    clockFastForward: (deltaMs) => renderer.clockFastForward(deltaMs),
    clockResume: () => renderer.clockResume(),
    focusElement: (id) => renderer.focusElement(id),
    blur: () => renderer.blur(),
  }
}

export function browserKeystrokeInit(
  keystroke: string,
  isHeld = false
): KeyboardEventInit {
  const parts = keystroke.split("-")
  const modifiers = new Set<string>()
  while (parts.length > 1) {
    const modifier = parts[0].toLowerCase()
    if (!["alt", "cmd", "ctrl", "meta", "shift"].includes(modifier)) break
    modifiers.add(modifier)
    parts.shift()
  }

  const keyName = parts.join("-")
  const key =
    {
      backspace: "Backspace",
      delete: "Delete",
      down: "ArrowDown",
      enter: "Enter",
      escape: "Escape",
      left: "ArrowLeft",
      right: "ArrowRight",
      space: " ",
      tab: "Tab",
      up: "ArrowUp",
    }[keyName.toLowerCase()] ?? keyName
  return {
    key,
    altKey: modifiers.has("alt"),
    bubbles: true,
    ctrlKey: modifiers.has("ctrl"),
    metaKey: modifiers.has("cmd") || modifiers.has("meta"),
    repeat: isHeld,
    shiftKey: modifiers.has("shift"),
  }
}

/**
 * The hidden element GPUI's browser platform appends to `<body>`.
 *
 * A GPUI web app has two event surfaces: the `<canvas>` takes pointer events,
 * and this element takes **every keyboard and IME event**. `gpui_web` attaches
 * its `keydown` / `keyup` listeners here, not to the window or the canvas, so
 * dispatching a synthetic `KeyboardEvent` at this element is the only way to
 * type into a browser GPUIX app.
 *
 * Match on the attribute alone. The element used to be an `<input>` and is now
 * a `<textarea>`, because a single-line input strips newlines from an assigned
 * value and would desynchronise the mirror from the document. See
 * zed-industries/zed#63201 and `gpui_web/src/ime_mirror.rs`. A tag-qualified
 * selector silently turns every keystroke into an "unavailable" error.
 */
export const IME_MIRROR_SELECTOR = "[data-gpui-input]"

function dispatchBrowserKeystroke({
  keystroke,
  type,
  isHeld = false,
}: {
  keystroke: string
  type: "keydown" | "keyup"
  isHeld?: boolean
}): void {
  const mirror = document.querySelector(IME_MIRROR_SELECTOR)
  if (!mirror) {
    throw new AutomationError(
      "Unsupported",
      `No GPUI keyboard target: nothing matches "${IME_MIRROR_SELECTOR}". ` +
        "Call this only after render() has painted a frame in a browser page."
    )
  }
  mirror.dispatchEvent(new KeyboardEvent(type, browserKeystrokeInit(keystroke, isHeld)))
}

export function browserRendererAsTest(
  renderer: LiveAutomationRenderer
): TestAutomationRenderer {
  const live = liveRendererAsTest(renderer)
  const keystrokes = (keys: string): void => {
    for (const key of keys.split(/\s+/).filter(Boolean)) {
      dispatchBrowserKeystroke({ keystroke: key, type: "keydown" })
      dispatchBrowserKeystroke({ keystroke: key, type: "keyup" })
    }
  }
  return {
    ...live,
    simulateKeystrokes: keystrokes,
    nativeSimulateKeystrokes(elementId, keys) {
      renderer.focusElement(elementId)
      keystrokes(keys)
    },
    nativeSimulateKeyDown(elementId, key, isHeld) {
      if (elementId > 0) renderer.focusElement(elementId)
      dispatchBrowserKeystroke({ keystroke: key, type: "keydown", isHeld })
    },
    nativeSimulateKeyUp(elementId, key) {
      if (elementId > 0) renderer.focusElement(elementId)
      dispatchBrowserKeystroke({ keystroke: key, type: "keyup" })
    },
  }
}

export async function connectTest(
  renderer: TestAutomationRenderer
): Promise<App> {
  const app = new App(new InProcessBackend(renderer))
  await app.call("initialize", {
    protocolVersion: PROTOCOL_VERSION,
    client: "@gpuix/native/automation",
  })
  return app
}

export async function connectStdio(options: {
  write: (chunk: string) => void
  feed: (listener: (chunk: string) => void) => void
  close?: () => Promise<void>
}): Promise<App> {
  const app = new App(
    new SseBackend(options.write, options.feed, options.close)
  )
  await app.call("initialize", {
    protocolVersion: PROTOCOL_VERSION,
    client: "@gpuix/native/automation",
  })
  return app
}

export async function launch(options: {
  command: string
  args?: string[]
  cwd?: string
  env?: NodeJS.ProcessEnv
}): Promise<App> {
  const { spawn } = await importNodeModule<typeof import("node:child_process")>(
    "node:child_process"
  )
  const child = spawn(
    options.command,
    options.args ?? [],
    {
      cwd: options.cwd,
      env: {
        ...process.env,
        ...options.env,
      },
      stdio: ["pipe", "pipe", "pipe"],
    }
  )
  const app = await connectStdio({
    write: (chunk) => {
      child.stdin.write(chunk)
    },
    feed: (listener) => {
      child.stdout.on("data", (buf: Buffer) => listener(buf.toString("utf8")))
    },
    close: async () => {
      child.kill()
    },
  })
  return app
}

export function handleAutomationRequest(
  raw: unknown,
  backend: AutomationBackend
): Promise<string> {
  const request = parseWireMessage(raw)
  if (!("method" in request)) {
    throw new AutomationError("Protocol", "Server expected a request")
  }
  return backend.call(request.method, request.params as never).then(
    (result) => encodeSse({ id: request.id, result } as AutomationResponse),
    (error) => {
      const failure =
        error instanceof AutomationError
          ? error
          : new AutomationError("Protocol", String(error))
      return encodeSse({
        id: request.id,
        error: {
          code: failure.code,
          message: failure.message,
          data: failure.data,
        },
      })
    }
  )
}

export function serveAutomationStdio(backend: AutomationBackend): void {
  const decoder = createSseDecoder((message) => {
    if (!("method" in message)) return
    void handleAutomationRequest(message, backend).then((reply) => {
      process.stdout.write(reply)
    })
  })
  process.stdin.setEncoding("utf8")
  process.stdin.on("data", (chunk: string) => {
    decoder.feed(chunk)
  })
}

export function isServerEvent(
  message: unknown
): message is AutomationServerEvent {
  return (
    typeof message === "object" &&
    message !== null &&
    "event" in message &&
    !("method" in message)
  )
}
