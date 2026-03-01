/// Host config for React's reconciler — mutation-based protocol.
///
/// Each reconciler callback (createInstance, appendChild, commitUpdate, etc.)
/// makes a direct napi call to the Rust retained tree. No JSON serialization
/// of the full element tree. Only changed elements cross the FFI boundary.

import { createContext } from "react"
import type { ReactContext } from "react-reconciler"
import { DefaultEventPriority } from "react-reconciler/constants"

const NoEventPriority = 0
import type {
  Container,
  ElementType,
  HostContext,
  Instance,
  NativeRenderer,
  Props,
  PublicInstance,
  TextInstance,
} from "../types/host"
import {
  registerEventHandler,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "./event-registry"

let elementIdCounter = 0
let currentUpdatePriority = NoEventPriority

// Renderer reference — set by createRoot before any reconciler work.
let nativeRenderer: NativeRenderer | null = null

export function setNativeRenderer(renderer: NativeRenderer): void {
  nativeRenderer = renderer
}

export function resetIdCounter(): void {
  elementIdCounter = 0
}

function nextId(): number {
  return ++elementIdCounter
}

function getRenderer(): NativeRenderer {
  if (!nativeRenderer) throw new Error("GPUIX renderer not set. Call createRoot first.")
  return nativeRenderer
}

// ── Event wiring helpers ─────────────────────────────────────────────

const EVENT_PROPS: Record<string, string> = {
  // Mouse events
  onClick: "click",
  onMouseDown: "mouseDown",
  onMouseUp: "mouseUp",
  onMouseEnter: "mouseEnter",
  onMouseLeave: "mouseLeave",
  onMouseMove: "mouseMove",
  onMouseDownOutside: "mouseDownOutside",
  // Keyboard events (require focus — tabIndex or autoFocus)
  onKeyDown: "keyDown",
  onKeyUp: "keyUp",
  // Focus events
  onFocus: "focus",
  onBlur: "blur",
  // Scroll events
  onScroll: "scroll",
}

function syncEventListeners(id: number, props: Props): void {
  const r = getRenderer()
  for (const [propName, eventType] of Object.entries(EVENT_PROPS)) {
    const handler = props[propName] as ((event: any) => void) | undefined
    if (handler) {
      registerEventHandler(id, eventType, handler)
      r.setEventListener(id, eventType, true)
    }
  }
}

function diffEventListeners(id: number, oldProps: Props, newProps: Props): void {
  const r = getRenderer()
  for (const [propName, eventType] of Object.entries(EVENT_PROPS)) {
    const oldHandler = oldProps[propName] as ((event: any) => void) | undefined
    const newHandler = newProps[propName] as ((event: any) => void) | undefined

    if (oldHandler && !newHandler) {
      // Removed — clean up both JS closure and Rust listener
      unregisterEventHandler(id, eventType)
      r.setEventListener(id, eventType, false)
    } else if (newHandler && newHandler !== oldHandler) {
      // Added or changed
      registerEventHandler(id, eventType, newHandler)
      if (!oldHandler) {
        r.setEventListener(id, eventType, true)
      }
    }
  }
}

// ── Style helper ─────────────────────────────────────────────────────

function sendStyle(id: number, props: Props): void {
  // Always send — handles style removal (undefined → {}) and avoids
  // missed updates from same-reference style objects.
  getRenderer().setStyle(id, JSON.stringify(props.style ?? {}))
}

// ── Custom prop forwarding ───────────────────────────────────────────

// Props that are handled by the reconciler directly (not forwarded as custom props).
const RESERVED_PROPS = new Set([
  "style",
  "className",
  "children",
  "key",
  "ref",
  "tabIndex",
  "tabStop",
  "autoFocus",
])

// Built-in element types that don't use custom props.
const BUILT_IN_TYPES = new Set(["div", "text"])

function isReservedProp(name: string): boolean {
  return RESERVED_PROPS.has(name) || name in EVENT_PROPS
}

function serializeCustomProp(
  type: string,
  key: string,
  value: unknown
): string {
  if (value === undefined) return "null"

  try {
    const json = JSON.stringify(value)
    if (json === undefined) return "null"
    return json
  } catch (error) {
    console.warn(
      `[gpuix] Failed to serialize custom prop ${type}.${key}; sending null`,
      error
    )
    return "null"
  }
}

/** Send all custom props to Rust for non-built-in element types. */
function syncCustomProps(id: number, type: string, props: Props): void {
  if (BUILT_IN_TYPES.has(type)) return
  const r = getRenderer()
  for (const [key, value] of Object.entries(props)) {
    if (isReservedProp(key)) continue
    r.setCustomProp(id, key, serializeCustomProp(type, key, value))
  }
}

/** Diff and send changed custom props to Rust. */
function diffCustomProps(
  id: number,
  type: string,
  oldProps: Props,
  newProps: Props
): void {
  if (BUILT_IN_TYPES.has(type)) return
  const r = getRenderer()
  // Updated or added props
  for (const [key, value] of Object.entries(newProps)) {
    if (isReservedProp(key)) continue
    if (oldProps[key] !== value) {
      r.setCustomProp(id, key, serializeCustomProp(type, key, value))
    }
  }
  // Removed props
  for (const key of Object.keys(oldProps)) {
    if (isReservedProp(key)) continue
    if (!(key in newProps)) {
      r.setCustomProp(id, key, JSON.stringify(null))
    }
  }
}

// ── Host config ──────────────────────────────────────────────────────

export const hostConfig = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,

  createInstance(
    type: ElementType,
    props: Props,
    _rootContainerInstance: Container,
    _hostContext: HostContext
  ): Instance {
    const id = nextId()
    const r = getRenderer()
    r.createElement(id, type)
    sendStyle(id, props)
    syncEventListeners(id, props)
    syncCustomProps(id, type, props)
    return { id, type, props }
  },

  appendChild(parent: Instance, child: Instance | TextInstance): void {
    getRenderer().appendChild(parent.id, child.id)
  },

  removeChild(parent: Instance, child: Instance | TextInstance): void {
    getRenderer().removeChild(parent.id, child.id)
  },

  insertBefore(
    parent: Instance,
    child: Instance | TextInstance,
    beforeChild: Instance | TextInstance
  ): void {
    getRenderer().insertBefore(parent.id, child.id, beforeChild.id)
  },

  insertInContainerBefore(
    _parent: Container,
    _child: Instance,
    _beforeChild: Instance
  ): void {},

  removeChildFromContainer(_parent: Container, child: Instance): void {
    const destroyed = getRenderer().destroyElement(child.id)
    for (const id of destroyed) {
      unregisterEventHandlers(id)
    }
  },

  prepareForCommit(_containerInfo: Container): Record<string, unknown> | null {
    return null
  },

  resetAfterCommit(_containerInfo: Container): void {
    getRenderer().commitMutations()
  },

  getRootHostContext(_rootContainerInstance: Container): HostContext {
    return { isInsideText: false }
  },

  getChildHostContext(
    parentHostContext: HostContext,
    type: ElementType,
    _rootContainerInstance: Container
  ): HostContext {
    const isInsideText = type === "text"
    return { ...parentHostContext, isInsideText }
  },

  shouldSetTextContent(_type: ElementType, _props: Props): boolean {
    return false
  },

  createTextInstance(
    text: string,
    _rootContainerInstance: Container,
    _hostContext: HostContext
  ): TextInstance {
    const id = nextId()
    const r = getRenderer()
    r.createElement(id, "text")
    r.setText(id, text)
    return { id, text, parentId: null }
  },

  scheduleTimeout: setTimeout,
  cancelTimeout: clearTimeout,
  noTimeout: -1,

  shouldAttemptEagerTransition(): boolean {
    return false
  },

  finalizeInitialChildren(
    _instance: Instance,
    _type: ElementType,
    _props: Props,
    _rootContainerInstance: Container,
    _hostContext: HostContext
  ): boolean {
    return false
  },

  commitMount(
    _instance: Instance,
    _type: ElementType,
    _props: Props,
    _internalInstanceHandle: unknown
  ): void {},

  commitUpdate(
    instance: Instance,
    _type: ElementType,
    oldProps: Props,
    newProps: Props,
    _internalInstanceHandle: unknown
  ): void {
    // Always resend style — per-element JSON is small, and this avoids
    // bugs from same-reference mutations or style removal.
    sendStyle(instance.id, newProps)
    // Event diff
    diffEventListeners(instance.id, oldProps, newProps)
    // Custom prop diff (for non-div/text elements)
    diffCustomProps(instance.id, instance.type, oldProps, newProps)
    instance.props = newProps
  },

  commitTextUpdate(
    textInstance: TextInstance,
    _oldText: string,
    newText: string
  ): void {
    getRenderer().setText(textInstance.id, newText)
    textInstance.text = newText
  },

  appendChildToContainer(container: Container, child: Instance): void {
    container.renderer.setRoot(child.id)
  },

  appendInitialChild(parent: Instance, child: Instance | TextInstance): void {
    getRenderer().appendChild(parent.id, child.id)
  },

  hideInstance(instance: Instance): void {
    getRenderer().setStyle(instance.id, JSON.stringify({ visibility: "hidden" }))
  },

  unhideInstance(instance: Instance, _props: Props): void {
    sendStyle(instance.id, instance.props)
  },

  hideTextInstance(_textInstance: TextInstance): void {},
  unhideTextInstance(_textInstance: TextInstance, _text: string): void {},

  clearContainer(_container: Container): void {},

  setCurrentUpdatePriority(newPriority: number): void {
    currentUpdatePriority = newPriority
  },

  getCurrentUpdatePriority: (): number => currentUpdatePriority,

  resolveUpdatePriority(): number {
    if (currentUpdatePriority !== NoEventPriority) {
      return currentUpdatePriority
    }
    return DefaultEventPriority
  },

  maySuspendCommit(): boolean {
    return false
  },

  NotPendingTransition: null,
  HostTransitionContext: createContext(null) as unknown as ReactContext<null>,
  resetFormInstance(): void {},
  requestPostPaintCallback(): void {},
  trackSchedulerEvent(): void {},

  resolveEventType(): null {
    return null
  },

  resolveEventTimeStamp(): number {
    return -1.1
  },

  preloadInstance(): boolean {
    return true
  },

  startSuspendingCommit(): void {},
  suspendInstance(): void {},

  waitForCommitToBeReady(): null {
    return null
  },

  detachDeletedInstance(instance: Instance): void {
    const destroyed = getRenderer().destroyElement(instance.id)
    for (const id of destroyed) {
      unregisterEventHandlers(id)
    }
  },

  getPublicInstance(instance: Instance): PublicInstance {
    return instance
  },

  preparePortalMount(_containerInfo: Container): void {},
  isPrimaryRenderer: true,

  getInstanceFromNode(): null {
    return null
  },

  beforeActiveInstanceBlur(): void {},
  afterActiveInstanceBlur(): void {},
  prepareScopeUpdate(): void {},

  getInstanceFromScope(): null {
    return null
  },
}
