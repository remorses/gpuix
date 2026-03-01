/// GPUIX TestRenderer — wraps the native TestGpuixRenderer (real GPUI pipeline)
/// with a local element map for test inspection.
///
/// All mutations go to BOTH the native Rust RetainedTree and a local JS map.
/// The native side runs the real GpuixView::render(), build_element(),
/// apply_styles(), and event wiring — same code as production.
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
}

// ── TestRenderer ─────────────────────────────────────────────────────

export class TestRenderer implements NativeRenderer {
  /** Local element map for test inspection (findByType, getElement, etc.). */
  elements = new Map<number, TestElement>()
  rootId: number | null = null
  commitCount = 0

  /** Native TestGpuixRenderer — runs real GPUI pipeline. Null if not available. */
  private native: import("@gpuix/native").TestGpuixRenderer | null = null

  constructor() {
    if (NativeTestRenderer) {
      this.native = new NativeTestRenderer()
    }
  }

  // ── NativeRenderer interface (mutations go to both native + local) ──

  createElement(id: number, elementType: string): void {
    this.native?.createElement(id, elementType)
    this.elements.set(id, {
      id,
      type: elementType,
      style: {},
      text: null,
      events: new Set(),
      children: [],
      parentId: null,
    })
  }

  destroyElement(id: number): Array<number> {
    this.native?.destroyElement(id)
    const destroyed: number[] = []
    const destroy = (eid: number) => {
      const el = this.elements.get(eid)
      if (!el) return
      destroyed.push(eid)
      for (const childId of el.children) {
        destroy(childId)
      }
      this.elements.delete(eid)
    }
    destroy(id)
    if (this.rootId === id) this.rootId = null
    return destroyed
  }

  appendChild(parentId: number, childId: number): void {
    this.native?.appendChild(parentId, childId)
    const child = this.elements.get(childId)
    if (child?.parentId != null) {
      const oldParent = this.elements.get(child.parentId)
      if (oldParent) {
        oldParent.children = oldParent.children.filter((c) => c !== childId)
      }
    }
    if (child) child.parentId = parentId
    const parent = this.elements.get(parentId)
    if (parent) parent.children.push(childId)
  }

  removeChild(parentId: number, childId: number): void {
    this.native?.removeChild(parentId, childId)
    const parent = this.elements.get(parentId)
    if (parent) {
      parent.children = parent.children.filter((c) => c !== childId)
    }
    const child = this.elements.get(childId)
    if (child) child.parentId = null
  }

  insertBefore(parentId: number, childId: number, beforeId: number): void {
    this.native?.insertBefore(parentId, childId, beforeId)
    const child = this.elements.get(childId)
    if (child?.parentId != null) {
      const oldParent = this.elements.get(child.parentId)
      if (oldParent) {
        oldParent.children = oldParent.children.filter((c) => c !== childId)
      }
    }
    if (child) child.parentId = parentId
    const parent = this.elements.get(parentId)
    if (parent) {
      const idx = parent.children.indexOf(beforeId)
      if (idx !== -1) {
        parent.children.splice(idx, 0, childId)
      } else {
        parent.children.push(childId)
      }
    }
  }

  setStyle(id: number, styleJson: string): void {
    this.native?.setStyle(id, styleJson)
    const el = this.elements.get(id)
    if (el) el.style = JSON.parse(styleJson)
  }

  setText(id: number, content: string): void {
    this.native?.setText(id, content)
    const el = this.elements.get(id)
    if (el) el.text = content
  }

  setEventListener(id: number, eventType: string, hasHandler: boolean): void {
    this.native?.setEventListener(id, eventType, hasHandler)
    const el = this.elements.get(id)
    if (!el) return
    if (hasHandler) {
      el.events.add(eventType)
    } else {
      el.events.delete(eventType)
    }
  }

  setRoot(id: number): void {
    this.native?.setRoot(id)
    this.rootId = id
  }

  commitMutations(): void {
    this.native?.commitMutations()
    this.commitCount++
  }

  // ── GPUI pipeline methods ───────────────────────────────────────

  /** Trigger the real GPUI rendering pipeline (GpuixView::render() →
   *  build_element() → apply_styles() → layout). No-op if native not available. */
  flush(): void {
    this.native?.flush()
  }

  /** Drain events collected by the native GPUI event handlers.
   *  Returns an empty array if native not available. */
  drainEvents(): EventPayload[] {
    return this.native?.drainEvents() ?? []
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
    if (!this.native) return
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
   *  dispatch resulting events to React. Requires native renderer.
   *  @param elementId - element to focus (must have onKeyDown/onKeyUp)
   *  @param keystrokes - space-separated keys, e.g. "a", "enter", "cmd-shift-p"
   */
  nativeSimulateKeystrokes(elementId: number, keystrokes: string): void {
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateKeystrokes")
    }
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
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateKeyDown")
    }
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
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateKeyUp")
    }
    this.native.flush()
    this.native.focusElement(elementId)
    this.native.simulateKeyUp(keystroke)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate a click through GPUI hit testing →
   *  dispatch resulting events to React. */
  nativeSimulateClick(x: number, y: number): void {
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateClick")
    }
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
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateScrollWheel")
    }
    this.native.flush()
    this.native.simulateScrollWheel(x, y, deltaX, deltaY)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse move through GPUI →
   *  dispatch resulting events to React.
   *  @param pressedButton - optional button held during move (0=left, 1=middle, 2=right) for drag simulation */
  nativeSimulateMouseMove(x: number, y: number, pressedButton?: number): void {
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateMouseMove")
    }
    this.native.flush()
    this.native.simulateMouseMove(x, y, pressedButton)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse down through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseDown(x: number, y: number, button?: number): void {
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateMouseDown")
    }
    this.native.flush()
    this.native.simulateMouseDown(x, y, button ?? 0)
    this.dispatchNativeEvents()
  }

  /** End-to-end: simulate mouse up through GPUI hit testing →
   *  dispatch resulting events to React.
   *  @param button - 0=left (default), 1=middle, 2=right */
  nativeSimulateMouseUp(x: number, y: number, button?: number): void {
    if (!this.native) {
      throw new Error("Native renderer not available for nativeSimulateMouseUp")
    }
    this.native.flush()
    this.native.simulateMouseUp(x, y, button ?? 0)
    this.dispatchNativeEvents()
  }

  // ── Tree inspection (reads from local element map) ─────────────

  /** Get the root element. */
  getRoot(): TestElement | undefined {
    return this.rootId != null ? this.elements.get(this.rootId) : undefined
  }

  /** Get an element by ID. */
  getElement(id: number): TestElement | undefined {
    return this.elements.get(id)
  }

  /** Find elements by type (e.g. "div", "text"). */
  findByType(type: string): TestElement[] {
    return [...this.elements.values()].filter((el) => el.type === type)
  }

  /** Find the first text element containing the given string. */
  findByText(text: string): TestElement | undefined {
    return [...this.elements.values()].find(
      (el) => el.text != null && el.text.includes(text)
    )
  }

  /** Get all text content in the tree (depth-first). */
  getAllText(): string[] {
    const texts: string[] = []
    const walk = (id: number) => {
      const el = this.elements.get(id)
      if (!el) return
      if (el.text != null) texts.push(el.text)
      for (const childId of el.children) {
        walk(childId)
      }
    }
    if (this.rootId != null) walk(this.rootId)
    return texts
  }

  /** Print the tree structure for debugging. Only includes non-empty fields. */
  toJSON(): unknown {
    const serialize = (id: number): Record<string, unknown> | null => {
      const el = this.elements.get(id)
      if (!el) return null
      const result: Record<string, unknown> = {
        type: el.type,
        id: el.id,
      }
      if (el.text != null) result.text = el.text
      if (Object.keys(el.style).length > 0) result.style = el.style
      if (el.events.size > 0) result.events = [...el.events].sort()
      if (el.children.length > 0)
        result.children = el.children
          .map(serialize)
          .filter(Boolean)
      return result
    }
    return this.rootId != null ? serialize(this.rootId) : null
  }

  /** Capture a screenshot of the current rendered UI and save as PNG.
   *  macOS only — requires Metal GPU rendering via VisualTestAppContext. */
  captureScreenshot(path: string): void {
    if (!this.native) {
      throw new Error("Native renderer not available for captureScreenshot")
    }
    this.native.flush()
    this.native.captureScreenshot(path)
  }

  /** Whether the native GPUI test renderer is available. */
  get hasNative(): boolean {
    return this.native != null
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
 * If built with test-support, mutations also go to the real GPUI pipeline.
 * Returns the Root (for rendering), the TestRenderer (for inspection/events),
 * and convenience methods.
 */
export function createTestRoot(): TestRoot {
  // Reset ID counter so tests are deterministic
  resetIdCounter()

  const renderer = new TestRenderer()
  setNativeRenderer(renderer)

  const gpuixContainer = { renderer }

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
    // Trigger GPUI rendering pipeline if native is available.
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
