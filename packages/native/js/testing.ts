/// GPUIX TestRenderer — thin wrapper over the native TestGpuixRenderer.
///
/// All state lives in Rust's RetainedTree. All mutations go directly to
/// the native renderer via napi. Inspection methods (findByType, getAllText,
/// toJSON, etc.) query the Rust tree via napi — no JS-side shadow copy.
///
/// All event simulation goes through the native GPUI pipeline (coordinate-based
/// hit testing, GPUI dispatch, emit_event_full). The nativeSimulate* methods
/// flush the tree, dispatch through GPUI, drain events, and feed them into
/// the injected framework event dispatcher.

import {
  hasTestGpuixRenderer,
  TestGpuixRenderer,
  type EventPayload,
} from "../index.js"
import type {
  DebugFrameOverlayMode,
  DebugFrameOverlayStats,
  ElementBounds,
  HighlightMatch,
  ImagePixelFormat,
  NativeRenderer,
  PathPromptOptions,
  WindowKeyEventHandlers,
} from "./host.js"
import { createRendererState } from "./renderer-state.js"
export {
  applyMacCpuThrottleFromEnv,
  MAC_CPU_THROTTLES,
  readMacCpuThrottle,
} from "./cpu-throttle.js"
export type { MacCpuThrottle } from "./cpu-throttle.js"

interface NativeTestRendererApi extends NativeRenderer {
  promptForPaths(options?: PathPromptOptions): Promise<string[] | null>
  flush(): void
  drainEvents(): EventPayload[]
  simulateKeystrokes(keystrokes: string): void
  focusElement(elementId: number): void
  focusNext(): void
  focusPrevious(): void
  getFocusedElementId(): number | null
  focusNextWithin(elementId: number): void
  focusPreviousWithin(elementId: number): void
  setWindowKeyEvents(keyDown: boolean, keyUp: boolean, eventId: number): void
  setWindowSelectionChange(enabled: boolean, eventId: number): void
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
  simulateMouseExit(x: number, y: number): void
  simulateMouseDown(x: number, y: number, button: number, modifiers?: string): void
  simulateMouseUp(x: number, y: number, button: number, modifiers?: string): void
  simulateFileDrop(x: number, y: number, paths: string[]): void
  getTreeJson(): string
  getA11yTree(): string
  getAutomationTree(): string
  getRetainedElementCount(): number
  getElementBounds(elementId: number): ElementBounds | null
  getElementsAt(x: number, y: number): number[]
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
  scrollIntoView(elementId: number): void
  setImage(elementId: number, bytes: Buffer | Uint8Array): void
  /** Make a TrueType or OpenType font usable by `fontFamily`. */
  registerFont(bytes: Buffer | Uint8Array): void
  setImagePixels(
    elementId: number,
    width: number,
    height: number,
    pixels: Buffer | Uint8Array,
    format?: ImagePixelFormat
  ): void
  getScrollOffset(elementId: number): number[] | null
  getListScrollTop(elementId: number): number[] | null
  setDebugFrameOverlay(mode: DebugFrameOverlayMode): string
  getDebugFrameOverlay(): string
  cycleDebugFrameOverlay(): string
  resetDebugFrameOverlayStats(): void
  getDebugFrameOverlayStats(): DebugFrameOverlayStats
  dragSelect(x1: number, y1: number, x2: number, y2: number): void
  getSelectedText(): string | null
  getPaintedText(): string[]
  getPaintedHighlights(): HighlightMatch[]
  getSyntaxCacheStats(): number[]
  clearSelection(): void
  captureScreenshot(path: string): void
}

interface NativeTestRendererConstructor {
  new (width?: number, height?: number): NativeTestRendererApi
}

/** Offscreen window size for a test renderer. Defaults to 1280x800 in native. */
export interface TestRendererOptions {
  width?: number
  height?: number
  /** Framework scheduling boundary for events drained from native. */
  dispatchEvent?: (event: EventPayload) => void
}

export type TestWindowOptions = TestRendererOptions & WindowKeyEventHandlers

// The Linux stub has no test methods; the native availability check guards the full API.
function hasNativeTestRendererClass(
  renderer: typeof TestGpuixRenderer
): renderer is typeof TestGpuixRenderer & NativeTestRendererConstructor {
  return hasTestGpuixRenderer()
}

const NativeTestRenderer: NativeTestRendererConstructor | null =
  hasNativeTestRendererClass(TestGpuixRenderer) ? TestGpuixRenderer : null

/** Whether the native TestGpuixRenderer is available (for conditional test registration). */
export const hasNativeTestRenderer = NativeTestRenderer != null

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

/** GPUI `debug_a11y_tree_json` dump. Node keys are ephemeral (`a`, `b`, …). */
export interface A11yTreeNode {
  aria?: {
    role?: string
    label?: string
    description?: string
    value?: string
    author_id?: string
    expanded?: boolean
    selected?: boolean
    level?: number
    /** `"True" | "False" | "Mixed"`, straight from AccessKit's `Toggled`. */
    toggled?: string
    /** `"Horizontal" | "Vertical"`, from AccessKit's `Orientation`. */
    orientation?: string
    keyboard_shortcut?: string
    placeholder?: string
    numeric_value?: number
    min_numeric_value?: number
    max_numeric_value?: number
    position_in_set?: number
    size_of_set?: number
    row_index?: number
    column_index?: number
    row_count?: number
    column_count?: number
    on_action?: string[]
  }
}

export interface A11yTreeDump {
  root?: string
  nodes?: Record<string, A11yTreeNode>
}

// ── TestRenderer ─────────────────────────────────────────────────────

export class TestRenderer implements NativeRenderer {
  /** Native TestGpuixRenderer — all state lives here in Rust's RetainedTree. */
  private native: NativeTestRendererApi
  readonly applyBatch: NativeRenderer["applyBatch"]
  readonly focusNext: () => void
  readonly focusPrevious: () => void
  readonly promptForPaths: NonNullable<NativeRenderer["promptForPaths"]>
  readonly setWindowSelectionChange: NonNullable<
    NativeRenderer["setWindowSelectionChange"]
  >
  readonly setWindowKeyEvents: NonNullable<NativeRenderer["setWindowKeyEvents"]>
  private readonly dispatchEvent: (event: EventPayload) => void

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
    this.promptForPaths = this.native.promptForPaths.bind(this.native)
    this.setWindowKeyEvents = this.native.setWindowKeyEvents.bind(this.native)
    this.setWindowSelectionChange = this.native.setWindowSelectionChange.bind(
      this.native
    )
    this.dispatchEvent = options.dispatchEvent ?? ((event) => {
      createRendererState(this).dispatch(event)
    })
  }

  // ── GPUI pipeline methods ───────────────────────────────────────

  /** Trigger the real GPUI rendering pipeline (GpuixView::render() →
   *  build_element() → apply_styles() → layout). */
  flush(): void {
    this.native.flush()
  }

  /** Drain events collected by the native GPUI event handlers. */
  drainEvents(): EventPayload[] {
    return this.native.drainEvents()
  }

  // ── Native end-to-end simulation ────────────────────────────────
  // These methods go through the full GPUI pipeline:
  //   native simulate → GPUI dispatch → hit test → event handler →
  //   emit_event_full → drainEvents → framework event handler

  /** Drain events from the native GPUI pipeline and feed them into the
   *  framework event registry, triggering state updates synchronously.
   *  Loops until no more events are produced — handles re-entrant events
   *  that may be generated during framework state updates. */
  dispatchNativeEvents(): void {
    for (;;) {
      const events = this.native.drainEvents()
      if (events.length === 0) break
      for (const event of events) {
        this.dispatchEvent(event)
      }
    }
  }

  /** End-to-end: focus element → simulate keystrokes through GPUI →
   *  dispatch resulting events to the framework.
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
   *  dispatch resulting events to the framework. Unlike nativeSimulateKeystrokes,
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
   *  dispatch resulting events to the framework. Pairs with nativeSimulateKeyDown.
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
   *  dispatch resulting events to the framework. */
  nativeSimulateClick(
    x: number,
    y: number,
    button?: number,
    modifiers?: string
  ): void {
    this.native.flush()
    this.native.simulateClick(x, y, button, modifiers)
    this.dispatchNativeEvents()
    // Flush again after framework state updates so the Rust RetainedTree
    // is fully rebuilt and GPUI has re-laid-out before any screenshot.
    this.native.flush()
  }

  /** End-to-end: simulate scroll wheel through GPUI →
   *  dispatch resulting events to the framework. */
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
   *  Call `flush()` yourself, or the sample is the framework update only and
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
   *  dispatch resulting events to the framework.
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
    // Flush again after framework state updates so hover styles are applied
    // and the Rust tree is current before any screenshot.
    this.native.flush()
  }

  /** End-to-end: the pointer leaves the window at (x, y), the way the platform
   *  reports it, with no mouse move after the last position inside. */
  nativeSimulateMouseExit(x: number, y: number): void {
    this.native.flush()
    this.native.simulateMouseExit(x, y)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  /** End-to-end: simulate mouse down through GPUI hit testing →
   *  dispatch resulting events to the framework.
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
   *  dispatch resulting events to the framework.
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

  /** End-to-end: Finder-style file drop through GPUI to `onFileDrop`. */
  nativeSimulateFileDrop(x: number, y: number, paths: string[]): void {
    this.native.flush()
    this.native.simulateFileDrop(x, y, paths)
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

  /** GPUI accessibility dump from the last painted frame. */
  getA11yTree(): A11yTreeDump {
    return JSON.parse(this.native.getA11yTree()) as A11yTreeDump
  }

  getAutomationTree(): string {
    return this.native.getAutomationTree()
  }

  /** Every element the native tree holds, reachable or not. `toJSON()` walks
   *  from the root, so only this can see a node that was detached and leaked. */
  getRetainedElementCount(): number {
    return this.native.getRetainedElementCount()
  }

  getElementBounds(elementId: number): ElementBounds | null {
    return this.native.getElementBounds(elementId)
  }

  /** Ids of the elements under the window point, topmost first. */
  getElementsAt(x: number, y: number): number[] {
    return this.native.getElementsAt(x, y)
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

  getFocusedElementId(): number | null {
    this.native.flush()
    return this.native.getFocusedElementId()
  }

  focusNextWithin(elementId: number): void {
    this.native.flush()
    this.native.focusNextWithin(elementId)
    this.dispatchNativeEvents()
  }

  focusPreviousWithin(elementId: number): void {
    this.native.flush()
    this.native.focusPreviousWithin(elementId)
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

  scrollIntoView(elementId: number): void {
    this.native.flush()
    this.native.scrollIntoView(elementId)
    this.dispatchNativeEvents()
    this.native.flush()
  }

  setImage(elementId: number, bytes: Buffer | Uint8Array): void {
    this.native.flush()
    this.native.setImage(elementId, bytes as Buffer)
    this.native.flush()
  }

  registerFont(bytes: Buffer | Uint8Array): void {
    this.native.registerFont(bytes as Buffer)
    this.native.flush()
  }

  setImagePixels(
    elementId: number,
    width: number,
    height: number,
    pixels: Buffer | Uint8Array,
    format?: ImagePixelFormat
  ): void {
    this.native.flush()
    this.native.setImagePixels(elementId, width, height, pixels as Buffer, format)
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

  /** Capture the current Metal or DirectX frame and save it as a PNG. */
  captureScreenshot(path: string): void {
    this.native.flush()
    this.native.captureScreenshot(path)
  }

  /** Whether the native GPUI test renderer is available. Always true. */
  get hasNative(): boolean {
    return true
  }
}
