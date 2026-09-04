/// GPUIX TestRenderer — thin wrapper over the native TestGpuixRenderer.
///
/// All state lives in Rust's RetainedTree. All mutations go directly to
/// the native renderer via napi. Inspection methods (findByType, getAllText,
/// toJSON, etc.) query the Rust tree via napi — no JS-side shadow copy.
///
/// All event simulation goes through the native GPUI pipeline (coordinate-based
/// hit testing, GPUI dispatch, emit_event_full). The nativeSimulate* methods
/// flush the tree, dispatch through GPUI, drain events, and feed them into
/// the React event registry via handleGpuixEvent.

import { createRequire } from "node:module"

import type { ReactNode } from "react"
import type { EventPayload } from "@gpuix/native"
import type {
  DebugFrameOverlayMode,
  DebugFrameOverlayStats,
  HighlightMatch,
  NativeRenderer,
  RootOptions,
} from "./types/host.js"
import { createRoot, flushSync, type Root } from "./reconciler/reconciler.js"
import { handleGpuixEvent } from "./reconciler/event-registry.js"
export {
  applyMacCpuThrottleFromEnv,
  MAC_CPU_THROTTLES,
  readMacCpuThrottle,
} from "./cpu-throttle.js"
export type { MacCpuThrottle } from "./cpu-throttle.js"

interface NativeTestRendererApi extends NativeRenderer {
  flush(): void
  drainEvents(): EventPayload[]
  simulateKeystrokes(keystrokes: string): void
  focusElement(elementId: number): void
  focusNext(): void
  focusPrevious(): void
  setWindowKeyEvents(keyDown: boolean, keyUp: boolean, eventId: number): void
  simulateKeyDown(keystroke: string, isHeld?: boolean): void
  simulateKeyUp(keystroke: string): void
  simulateClick(x: number, y: number, button?: number, modifiers?: string): void
  simulateScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
    modifiers?: string
  ): void
  simulateMouseMove(
    x: number,
    y: number,
    pressedButton?: number,
    modifiers?: string
  ): void
  simulateMouseDown(x: number, y: number, button: number, modifiers?: string): void
  simulateMouseUp(x: number, y: number, button: number, modifiers?: string): void
  getTreeJson(): string
  getAutomationTree(): string
  getRetainedElementCount(): number
  getElementBounds(elementId: number): number[] | null
  clockPause(): number
  clockSet(nowMs: number): number
  clockFastForward(deltaMs: number): number
  clockResume(): number
  advanceTime(milliseconds: number): void
  getRootId(): number | null
  getWindowSize(): { width: number; height: number }
  getAllText(): string[]
  scrollTo(elementId: number, x: number, y: number): void
  scrollToItem(elementId: number, index: number, offsetInItem?: number): void
  scrollIntoView(elementId: number, block?: string, inline?: string): void
  getScrollOffset(elementId: number): number[] | null
  viewTransitionCapture(): void
  viewTransitionStart(options?: string): void
  getListScrollTop(elementId: number): number[] | null
  setDebugFrameOverlay(mode: DebugFrameOverlayMode): string
  getDebugFrameOverlay(): string
  cycleDebugFrameOverlay(): string
  resetDebugFrameOverlayStats(): void
  getDebugFrameOverlayStats(): DebugFrameOverlayStats
  styleResolutions(): number
  resetStyleResolutions(): void
  dragSelect(x1: number, y1: number, x2: number, y2: number): void
  getSelectedText(): string | null
  readClipboardText(): string | null
  getPaintedText(): string[]
  getPaintedHighlights(): HighlightMatch[]
  getSyntaxCacheStats(): number[]
  clearSelection(): void
  captureScreenshot(path: string): void
  pixelAt(x: number, y: number): number[]
}

interface NativeTestRendererConstructor {
  new (width?: number, height?: number): NativeTestRendererApi
}

/** Offscreen window size for a test renderer. Defaults to 1280x800 in native. */
export interface TestRendererOptions {
  width?: number
  height?: number
}

export type TestWindowOptions = TestRendererOptions & RootOptions

// The class is always exported. hasTestGpuixRenderer is the real GPU impl.
//
// Loaded through `createRequire`, never a bare `require`. This file ships as
// ESM, and Node has no `require` there: in a workspace vitest inlines it and
// happens to provide one, but a real dependency is externalized and run by
// Node, where the bare call threw `require is not defined`. The `catch` then
// made `hasNativeTestRenderer` false, so every suite that guards on it
// silently skipped for anyone consuming the published package.
let NativeTestRenderer: NativeTestRendererConstructor | null = null
try {
  const native = createRequire(import.meta.url)("@gpuix/native") as {
    TestGpuixRenderer?: NativeTestRendererConstructor
    hasTestGpuixRenderer?: () => boolean
  }
  if (native.hasTestGpuixRenderer?.() && native.TestGpuixRenderer) {
    NativeTestRenderer = native.TestGpuixRenderer
  }
} catch {
  // Native module not available — native simulation methods will throw.
}

/** Whether the native TestGpuixRenderer is available (for conditional test registration). */
export const hasNativeTestRenderer = NativeTestRenderer != null

/// The env overrides the Rust side reads with `std::env::var`.
const NATIVE_ENV_OVERRIDES = ["GPUIX_SCROLLBARS"] as const

/**
 * Copies the env overrides from `process.env` into the real environment.
 *
 * Node writes a `process.env` assignment through to `setenv`, but Bun only
 * updates its JS snapshot, so under `bun test` the Rust side cannot see a
 * `process.env.GPUIX_SCROLLBARS = "classic"` from a test. This runs before
 * every frame flush to push the current values across.
 */
// Resolved once. The module registry caches the require, but this also
// skips the try/catch and the property read on every flush.
let nativeSyncEnvVar: ((key: string, value?: string) => void) | undefined
try {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  nativeSyncEnvVar = (
    require("@gpuix/native") as { syncEnvVar?: typeof nativeSyncEnvVar }
  ).syncEnvVar
} catch {
  // Native module not available. Nothing to sync.
}

function syncEnvOverrides(): void {
  if (!nativeSyncEnvVar) return
  for (const key of NATIVE_ENV_OVERRIDES) {
    nativeSyncEnvVar(key, process.env[key])
  }
}

// ── Test element tree ────────────────────────────────────────────────

export interface TestElement {
  id: number
  type: string
  style: Record<string, unknown>
  text: string | null
  events: Set<string>
  children: number[]
  parentId: number | null
  testId?: string
  customProps?: Record<string, unknown>
}

// ── TestRenderer ─────────────────────────────────────────────────────

export class TestRenderer implements NativeRenderer {
  /** Native TestGpuixRenderer — all state lives here in Rust's RetainedTree. */
  private native: NativeTestRendererApi
  readonly applyBatch: NativeRenderer["applyBatch"]
  readonly focusNext: () => void
  readonly focusPrevious: () => void
  readonly setWindowKeyEvents: (
    keyDown: boolean,
    keyUp: boolean,
    eventId: number
  ) => void

  constructor(options: TestRendererOptions = {}) {
    if (!NativeTestRenderer) {
      throw new Error(
        "TestGpuixRenderer is macOS and Windows only. Linux builds have no test-support because wgpu cannot read a rendered image back yet. GpuixRenderer still works on Linux."
      )
    }
    this.native = new NativeTestRenderer(options.width, options.height)
    this.applyBatch = this.native.applyBatch.bind(this.native)
    this.focusNext = this.native.focusNext.bind(this.native)
    this.focusPrevious = this.native.focusPrevious.bind(this.native)
    this.setWindowKeyEvents = this.native.setWindowKeyEvents.bind(this.native)
  }

  // ── GPUI pipeline methods ───────────────────────────────────────

  /** Trigger the real GPUI rendering pipeline (GpuixView::render() →
   *  build_element() → apply_styles() → layout). */
  flush(): void {
    syncEnvOverrides()
    this.native.flush()
  }

  /** Drain events collected by the native GPUI event handlers. */
  drainEvents(): EventPayload[] {
    return this.native.drainEvents()
  }

  // ── Native end-to-end simulation ────────────────────────────────
  // These methods go through the full GPUI pipeline:
  //   native simulate → GPUI dispatch → hit test → event handler →
  //   emit_event_full → drainEvents → handleGpuixEvent → React handler

  /** Drain events from the native GPUI pipeline and feed them into the
   *  React event registry, triggering state updates synchronously.
   *  Loops until no more events are produced — handles re-entrant events
   *  that may be generated during React state updates. */
  dispatchNativeEvents(): void {
    for (;;) {
      const events = this.native.drainEvents()
      if (events.length === 0) break
      for (const event of events) {
        flushSync(() => {
          handleGpuixEvent(event, this)
        })
      }
    }
  }

  /** End-to-end: focus element → simulate keystrokes through GPUI →
   *  dispatch resulting events to React.
   *  @param elementId - element to focus (must have onKeyDown/onKeyUp)
   *  @param keystrokes - space-separated keys, e.g. "a", "enter", "cmd-shift-p"
   */
  /** Send keystrokes to whatever currently holds focus.
   *
   *  Unlike `nativeSimulateKeystrokes`, this focuses nothing first, which is
   *  the only way to test that `autoFocus` (or a click) actually moved focus. */
  simulateKeystrokes(keystrokes: string): void {
    this.native.flush()
    this.native.simulateKeystrokes(keystrokes)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  nativeSimulateKeystrokes(elementId: number, keystrokes: string): void {
    this.native.flush()
    this.native.focusElement(elementId)
    this.native.simulateKeystrokes(keystrokes)
    this.dispatchNativeEvents()
  }

  /** End-to-end: focus element → simulate a single key down through GPUI →
   *  dispatch resulting events to React. Unlike nativeSimulateKeystrokes,
   *  this dispatches ONLY a KeyDownEvent — no automatic KeyUpEvent follows.
   *  @param elementId - element to focus (must have onKeyDown)
   *  @param keystroke - modifier-key string, e.g. "a", "enter", "cmd-s"
   *  @param isHeld - whether this is a key-repeat event (default: false)
   */
  nativeSimulateKeyDown(elementId: number, keystroke: string, isHeld?: boolean): void {
    this.native.flush()
    this.native.focusElement(elementId)
    this.native.simulateKeyDown(keystroke, isHeld)
    this.dispatchNativeEvents()
  }

  /** End-to-end: focus element → simulate a single key up through GPUI →
   *  dispatch resulting events to React. Pairs with nativeSimulateKeyDown.
   *  @param elementId - element to focus (must have onKeyUp)
   *  @param keystroke - modifier-key string, e.g. "a", "enter", "cmd-s"
   */
  nativeSimulateKeyUp(elementId: number, keystroke: string): void {
    this.native.flush()
    this.native.focusElement(elementId)
    this.native.simulateKeyUp(keystroke)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate a click through GPUI hit testing →
   *  dispatch resulting events to React. */
  nativeSimulateClick(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateClick(x, y, button, modifiers)
    this.dispatchNativeEvents()
    // Flush again after React state updates so the Rust RetainedTree
    // is fully rebuilt and GPUI has re-laid-out before any screenshot.
    this.native.flush()
  }

  /** End-to-end: simulate scroll wheel through GPUI →
   *  dispatch resulting events to React. */
  nativeSimulateScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateScrollWheel(x, y, deltaX, deltaY, modifiers)
    this.dispatchNativeEvents()
  }

  /** Dispatch a wheel without the surrounding flushes, for perf sampling.
   *  Call `flush()` yourself, or the sample is the React update only and
   *  none of the GPUI build, layout and paint that follows. */
  dispatchScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number,
    modifiers?: string
  ): void {
    this.native.simulateScrollWheel(x, y, deltaX, deltaY, modifiers)
    this.dispatchNativeEvents()
  }

  /** Dispatch a move without the surrounding flushes, for perf sampling.
   *  `nativeSimulateMouseMove` flushes before and after, so a drag timed with
   *  it contains two complete paints and cannot be compared to a wheel. */
  dispatchMouseMove(
    x: number,
    y: number,
    pressedButton?: number,
    modifiers?: string
  ): void {
    this.native.simulateMouseMove(x, y, pressedButton, modifiers)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse move through GPUI →
   *  dispatch resulting events to React.
   *  @param pressedButton - optional button held during move (0=left, 1=middle, 2=right) for drag simulation */
  nativeSimulateMouseMove(
    x: number,
    y: number,
    pressedButton?: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateMouseMove(x, y, pressedButton, modifiers)
    this.dispatchNativeEvents()
    // Flush again after React state updates so hover styles are applied
    // and the Rust tree is current before any screenshot.
    this.native.flush()
  }

  /** End-to-end: simulate mouse down through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseDown(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateMouseDown(x, y, button ?? 0, modifiers)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  /** End-to-end: simulate mouse up through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseUp(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateMouseUp(x, y, button ?? 0, modifiers)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  // ── Tree inspection (queries Rust RetainedTree via napi) ────────

  /** Build a flat map of TestElements from the native tree JSON.
   *  One FFI call to get the full tree, then parse into TestElement objects. */
  private buildElementMap(): Map<number, TestElement> {
    const json = JSON.parse(this.native.getTreeJson())
    const map = new Map<number, TestElement>()
    const walk = (node: any, parentId: number | null) => {
      if (!node) return
      map.set(node.id, {
        id: node.id,
        type: node.type,
        style: node.style ?? {},
        text: node.text ?? null,
        events: new Set(node.events ?? []),
        children: (node.children ?? []).map((c: any) => c.id),
        parentId,
        ...(node.testId ? { testId: node.testId } : {}),
        ...(node.customProps ? { customProps: node.customProps } : {}),
      })
      for (const child of node.children ?? []) {
        walk(child, node.id)
      }
    }
    walk(json, null)
    return map
  }

  /** Get the root element. */
  getRoot(): TestElement | undefined {
    const rootId = this.native.getRootId()
    if (rootId == null) return undefined
    return this.buildElementMap().get(rootId)
  }

  /** Get an element by ID. */
  getElement(id: number): TestElement | undefined {
    return this.buildElementMap().get(id)
  }

  /** Find elements by type (e.g. "div", "text"). */
  findByType(type: string): TestElement[] {
    return [...this.buildElementMap().values()].filter((el) => el.type === type)
  }

  /** Find the first text element containing the given string. */
  findByText(text: string): TestElement | undefined {
    return [...this.buildElementMap().values()].find(
      (el) => el.text != null && el.text.includes(text)
    )
  }

  findByTestId(testId: string): TestElement | undefined {
    return [...this.buildElementMap().values()].find((el) => el.testId === testId)
  }

  /** Get all text content in the tree (depth-first). */
  getAllText(): string[] {
    return this.native.getAllText()
  }

  /** Print the tree structure for debugging. Only includes non-empty fields. */
  toJSON(): unknown {
    return JSON.parse(this.native.getTreeJson())
  }

  getAutomationTree(): string {
    return this.native.getAutomationTree()
  }

  /** Every element the native tree holds, reachable or not. `toJSON()` walks
   *  from the root, so only this can see a node that was detached and leaked. */
  getRetainedElementCount(): number {
    return this.native.getRetainedElementCount()
  }

  getElementBounds(elementId: number): number[] | null {
    return this.native.getElementBounds(elementId)
  }

  clockPause(): number {
    return this.native.clockPause()
  }

  clockSet(nowMs: number): number {
    return this.native.clockSet(nowMs)
  }

  clockFastForward(deltaMs: number): number {
    return this.native.clockFastForward(deltaMs)
  }

  clockResume(): number {
    return this.native.clockResume()
  }

  /** Advance GPUI's test dispatcher and run due timers.
   *  This is not `clockFastForward`. That moves the motion clock only.
   *  Use this for caret blink, input drag autoscroll, and list edge scroll. */
  advanceTime(milliseconds: number): void {
    this.native.advanceTime(milliseconds)
    this.dispatchNativeEvents()
  }

  focusElement(elementId: number): void {
    this.native.flush()
    this.native.focusElement(elementId)
    this.dispatchNativeEvents()
  }

  /** The offscreen window size, so `useWindowSize()` works under test. */
  getWindowSize(): { width: number; height: number } {
    return this.native.getWindowSize()
  }

  // ── Scroll API ──────────────────────────────────────────────────

  /** Set the scroll offset of a scrollable element (overflow: "scroll").
   *  x and y are negative pixel values (scroll down = more negative y).
   *  Call flush() internally to apply. */
  scrollTo(elementId: number, x: number, y: number): void {
    this.native.flush()
    this.native.scrollTo(elementId, x, y)
    // Flush again to re-render with the new offset
    this.native.flush()
  }

  /** Scroll a child into view by its index in the children list.
   *
   *  `offsetInItem` is in pixels. A negative value anchors the viewport top
   *  above the item, resolved against measured row heights at layout time, so
   *  a row stays pixel-stable while unmeasured rows are spliced in above it. */
  scrollToItem(elementId: number, index: number, offsetInItem?: number): void {
    this.native.flush()
    this.native.scrollToItem(elementId, index, offsetInItem)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  /** Scroll every ancestor scroll box so the element shows, like the web
   *  scrollIntoView. block places it on the y axis and inline on the x
   *  axis: "start", "center", "end" or "nearest". The defaults match the
   *  web: "start" and "nearest". scrollMargin on the element and
   *  scrollPadding on a box apply. */
  scrollIntoView(elementId: number, block?: string, inline?: string): void {
    this.native.flush()
    this.native.scrollIntoView(elementId, block, inline)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  /** Get the current scroll offset [x, y] or null if element is not scrollable. */
  getScrollOffset(elementId: number): [number, number] | null {
    this.native.flush()
    const result = this.native.getScrollOffset(elementId)
    if (!result) return null
    return [result[0], result[1]]
  }

  /** The logical scroll anchor of a `<virtual-list>`:
   *  `[itemIndex, offsetInItemPx, viewportHeightPx]`, or null for anything
   *  else. `itemIndex == item count` is gpui's at-end sentinel. Exact even
   *  while row heights are still estimates, because it is the anchor gpui
   *  itself scrolls by. */
  getListScrollTop(elementId: number): [number, number, number] | null {
    this.native.flush()
    const result = this.native.getListScrollTop(elementId)
    if (!result) return null
    return [result[0], result[1], result[2]]
  }

  // ── View transitions ────────────────────────────────────────────

  /** Clone every element that has a `viewTransitionName`, with its painted
   *  bounds. The flush first makes those bounds current. */
  viewTransitionCapture(): void {
    this.native.flush()
    this.native.viewTransitionCapture()
  }

  /** Animate every captured name toward its new element. Pause the clock
   *  first and move it to step through the frames. */
  viewTransitionStart(options?: string): void {
    this.native.viewTransitionStart(options)
    this.native.flush()
  }

  // ── Selection API ───────────────────────────────────────────────

  /** Drag-select from (x1,y1) to (x2,y2) and return the selected text.
   *
   *  Selection listeners are registered during **paint**, so the native helper
   *  flushes between every step. Calling simulateMouseDown/Move/Up by hand
   *  without those flushes selects nothing. */
  dragSelect(x1: number, y1: number, x2: number, y2: number): string | null {
    this.native.dragSelect(x1, y1, x2, y2)
    return this.native.getSelectedText()
  }

  /** The current selection joined in document order, or null. */
  getSelectedText(): string | null {
    return this.native.getSelectedText()
  }

  /// The text on the clipboard after a copy, or null when the clipboard has no text.
  readClipboardText(): string | null {
    return this.native.readClipboardText()
  }

  /** Every string painted in the last frame, in paint order.
   *
   *  `getAllText()` only sees `<text>` nodes in the retained tree. Native
   *  elements like `<code>` and `<diff>` paint their text inside GPUI, so this
   *  is the only way to assert on what they rendered. */
  getPaintedText(): string[] {
    return this.native.getPaintedText()
  }

  /** Every highlight wash painted in the last frame, in paint order.
   *
   *  A quad never lands in `getPaintedText()`, and a soft-wrapped match must
   *  draw one box per visual row, so each entry carries its `rects`. */
  getPaintedHighlights(): HighlightMatch[] {
    return this.native.getPaintedHighlights()
  }

  /** Syntax-cache counters as `[hits, misses, documents]`. */
  getSyntaxCacheStats(): [number, number, number] {
    const [hits, misses, documents] = this.native.getSyntaxCacheStats()
    return [hits, misses, documents]
  }

  clearSelection(): void {
    this.native.clearSelection()
    this.native.flush()
  }

  setDebugFrameOverlay(mode: DebugFrameOverlayMode): string {
    return this.native.setDebugFrameOverlay(mode)
  }

  getDebugFrameOverlay(): string {
    return this.native.getDebugFrameOverlay()
  }

  cycleDebugFrameOverlay(): string {
    return this.native.cycleDebugFrameOverlay()
  }

  resetDebugFrameOverlayStats(): void {
    this.native.resetDebugFrameOverlayStats()
  }

  getDebugFrameOverlayStats(): DebugFrameOverlayStats {
    return this.native.getDebugFrameOverlayStats()
  }

  /** How many styles the renderer resolved since the last reset.
   *  A frame that changes nothing must not raise this. */
  styleResolutions(): number {
    return this.native.styleResolutions()
  }

  resetStyleResolutions(): void {
    this.native.resetStyleResolutions()
  }

  /** Capture the current Metal or DirectX frame and save it as a PNG. */
  captureScreenshot(path: string): void {
    this.native.flush()
    this.native.captureScreenshot(path)
  }

  /** The painted colour at a logical pixel, as `[r, g, b, a]` from 0 to 255. */
  pixelAt(x: number, y: number): [number, number, number, number] {
    this.native.flush()
    const [r, g, b, a] = this.native.pixelAt(x, y)
    return [r, g, b, a]
  }

  /** Whether the native GPUI test renderer is available. Always true. */
  get hasNative(): boolean {
    return true
  }
}

// ── Test root helper ─────────────────────────────────────────────────

export interface TestRoot {
  root: Root
  renderer: TestRenderer
  render: (node: ReactNode) => void
  unmount: () => void
}

/**
 * Create a test root for rendering React components.
 * All mutations go to the real GPUI pipeline via native TestGpuixRenderer.
 * Returns the Root (for rendering), the TestRenderer (for inspection/events),
 * and convenience methods.
 *
 * Pass `width` / `height` to size the offscreen window. The 1280x800 default is
 * wide enough to keep a centered max-width column capped, so a layout test that
 * needs to observe re-wrapping must ask for a narrower window.
 */
export function createTestRoot(options: TestWindowOptions = {}): TestRoot {
  const renderer = new TestRenderer(options)
  const root = createRoot(renderer, options)

  const render = (node: ReactNode): void => {
    flushSync(() => root.render(node))
    // Trigger GPUI rendering pipeline after the synchronous React commit.
    renderer.flush()
  }

  return {
    root,
    renderer,
    render,
    unmount: root.unmount,
  }
}
