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

import React from "react"
import type { ReactNode } from "react"
import type { EventPayload } from "@gpuix/native"
import type { NativeRenderer } from "./types/host"
import type { Root } from "./reconciler/renderer"
import { reconciler } from "./reconciler/reconciler"
import { setNativeRenderer, resetIdCounter } from "./reconciler/host-config"
import { clearEventHandlers, handleGpuixEvent } from "./reconciler/event-registry"
import { wrapWithBatching } from "./reconciler/batch-renderer"
import type { OpaqueRoot } from "react-reconciler"
import { ConcurrentRoot } from "react-reconciler/constants"

// Try to load the native TestGpuixRenderer (only available when built with test-support).
let NativeTestRenderer: (new () => import("@gpuix/native").TestGpuixRenderer) | null = null
try {
  // eslint-disable-next-line @typescript-eslint/no-require-imports
  const native = require("@gpuix/native")
  if (native.TestGpuixRenderer) {
    NativeTestRenderer = native.TestGpuixRenderer
  }
} catch {
  // Native module not available — native simulation methods will throw.
}

/** Whether the native TestGpuixRenderer is available (for conditional test registration). */
export const hasNativeTestRenderer = NativeTestRenderer != null

// Access reconciler.flushSync (name varies by version)
const _r = reconciler as typeof reconciler & {
  flushSyncFromReconciler?: typeof reconciler.flushSync
}
const flushSync = _r.flushSyncFromReconciler ?? _r.flushSync

// ── Test element tree ────────────────────────────────────────────────

export interface TestElement {
  id: number
  type: string
  style: Record<string, unknown>
  text: string | null
  events: Set<string>
  children: number[]
  parentId: number | null
  customProps?: Record<string, unknown>
}

// ── TestRenderer ─────────────────────────────────────────────────────

export class TestRenderer implements NativeRenderer {
  commitCount = 0

  /** Native TestGpuixRenderer — all state lives here in Rust's RetainedTree. */
  private native: import("@gpuix/native").TestGpuixRenderer

  constructor() {
    if (!NativeTestRenderer) {
      throw new Error(
        "Native TestGpuixRenderer not available. Build with test-support to run tests."
      )
    }
    this.native = new NativeTestRenderer()
  }

  // ── NativeRenderer interface (all mutations delegate to native) ──

  createElement(id: number, elementType: string): void {
    this.native.createElement(id, elementType)
  }

  destroyElement(id: number): Array<number> {
    return this.native.destroyElement(id)
  }

  appendChild(parentId: number, childId: number): void {
    this.native.appendChild(parentId, childId)
  }

  removeChild(parentId: number, childId: number): void {
    this.native.removeChild(parentId, childId)
  }

  insertBefore(parentId: number, childId: number, beforeId: number): void {
    this.native.insertBefore(parentId, childId, beforeId)
  }

  setStyle(id: number, styleJson: string): void {
    this.native.setStyle(id, styleJson)
  }

  setText(id: number, content: string): void {
    this.native.setText(id, content)
  }

  setEventListener(id: number, eventType: string, hasHandler: boolean): void {
    this.native.setEventListener(id, eventType, hasHandler)
  }

  setRoot(id: number): void {
    this.native.setRoot(id)
  }

  setCustomProp(id: number, key: string, valueJson: string): void {
    this.native.setCustomProp(id, key, valueJson)
  }

  commitMutations(): void {
    this.native.commitMutations()
    this.commitCount++
  }

  applyBatch(json: string): Array<number> {
    return this.native.applyBatch(json)
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
          handleGpuixEvent(event)
        })
      }
    }
  }

  /** End-to-end: focus element → simulate keystrokes through GPUI →
   *  dispatch resulting events to React.
   *  @param elementId - element to focus (must have onKeyDown/onKeyUp)
   *  @param keystrokes - space-separated keys, e.g. "a", "enter", "cmd-shift-p"
   */
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
  nativeSimulateClick(x: number, y: number): void {
    this.native.flush()
    this.native.simulateClick(x, y)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate scroll wheel through GPUI →
   *  dispatch resulting events to React. */
  nativeSimulateScrollWheel(
    x: number,
    y: number,
    deltaX: number,
    deltaY: number
  ): void {
    this.native.flush()
    this.native.simulateScrollWheel(x, y, deltaX, deltaY)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse move through GPUI →
   *  dispatch resulting events to React.
   *  @param pressedButton - optional button held during move (0=left, 1=middle, 2=right) for drag simulation */
  nativeSimulateMouseMove(x: number, y: number, pressedButton?: number): void {
    this.native.flush()
    this.native.simulateMouseMove(x, y, pressedButton)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse down through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseDown(x: number, y: number, button?: number): void {
    this.native.flush()
    this.native.simulateMouseDown(x, y, button ?? 0)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse up through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseUp(x: number, y: number, button?: number): void {
    this.native.flush()
    this.native.simulateMouseUp(x, y, button ?? 0)
    this.dispatchNativeEvents()
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

  /** Get all text content in the tree (depth-first). */
  getAllText(): string[] {
    return this.native.getAllText()
  }

  /** Print the tree structure for debugging. Only includes non-empty fields. */
  toJSON(): unknown {
    return JSON.parse(this.native.getTreeJson())
  }

  /** Capture a screenshot of the current rendered UI and save as PNG.
   *  macOS only — requires Metal GPU rendering via VisualTestAppContext. */
  captureScreenshot(path: string): void {
    this.native.flush()
    this.native.captureScreenshot(path)
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
 */
export function createTestRoot(): TestRoot {
  // Reset ID counter so tests are deterministic
  resetIdCounter()

  const renderer = new TestRenderer()
  // Wrap with batching — mutations are buffered and sent in one applyBatch()
  // call per commit, same as production. Tests exercise the batching path.
  const batchedRenderer = wrapWithBatching(renderer)
  setNativeRenderer(batchedRenderer)

  const gpuixContainer = { renderer: batchedRenderer }

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const container: OpaqueRoot = (reconciler.createContainer as any)(
    gpuixContainer,
    ConcurrentRoot,
    null,
    false,
    null,
    "",
    console.error,
    console.error,
    console.error,
    null
  )

  const render = (node: ReactNode) => {
    // Wrap in flushSync so updates are applied synchronously for tests
    flushSync(() => {
      clearEventHandlers()
      reconciler.updateContainer(
        React.createElement(React.Fragment, null, node),
        container,
        null,
        () => {}
      )
    })
    // Trigger GPUI rendering pipeline.
    renderer.flush()
  }

  const unmount = () => {
    reconciler.updateContainer(null, container, null, () => {})
    // @ts-expect-error types not up to date
    reconciler.flushSyncWork?.()
    clearEventHandlers()
  }

  return {
    root: { render, unmount },
    renderer,
    render,
    unmount,
  }
}
