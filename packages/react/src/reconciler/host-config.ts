/// Host config for React's reconciler — mutation-based protocol.
///
/// Each reconciler callback (createInstance, appendChild, commitUpdate, etc.)
/// makes a direct napi call to the Rust retained tree. No JSON serialization
/// of the full element tree. Only changed elements cross the FFI boundary.

import { createContext } from "react"
import { DefaultEventPriority } from "react-reconciler/constants.js"

const NoEventPriority = 0
import type {
  Container,
  ElementType,
  HostContext,
  Instance,
  MutationRenderer,
  Props,
  PublicInstance,
  StyleDesc,
  TextInstance,
} from "../types/host.js"
import { styleForClassName, withInlineStyle } from "./class-names.js"
import {
  registerEventHandler,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "./event-registry.js"

let currentUpdatePriority = NoEventPriority

type HostNode = Instance | TextInstance

interface HostNodeState {
  container: Container
  initialChildren: HostNode[]
  mounted: boolean
}

const hostNodeStates = new WeakMap<HostNode, HostNodeState>()

function stateFor(node: HostNode): HostNodeState {
  const state = hostNodeStates.get(node)
  if (!state) {
    throw new Error(`GPUIX host node ${node.id} does not belong to a root`)
  }
  return state
}

function containerFor(node: HostNode): Container {
  return stateFor(node).container
}

function rendererFor(node: HostNode): MutationRenderer {
  return containerFor(node).renderer
}

function nextId(container: Container): number {
  return ++container.ids.nextElementId
}

// ── Event wiring helpers ─────────────────────────────────────────────

const EVENT_PROPS = [
  // Custom element events
  ["onToggleFile", "toggleFile"],
  ["onShowMore", "showMore"],
  ["onLineClick", "lineClick"],
  ["onLinkClick", "linkClick"],
  ["onVisibleRange", "visibleRange"],
  ["onHighlight", "highlight"],
  ["onChange", "change"],
  ["onSubmit", "submit"],
  // Mouse events
  ["onClick", "click"],
  ["onAuxClick", "auxClick"],
  ["onMouseDown", "mouseDown"],
  ["onMouseUp", "mouseUp"],
  ["onMouseEnter", "mouseEnter"],
  ["onMouseLeave", "mouseLeave"],
  ["onMouseMove", "mouseMove"],
  ["onMouseDownOutside", "mouseDownOutside"],
  // Keyboard events (require focus — tabIndex or autoFocus)
  ["onKeyDown", "keyDown"],
  ["onKeyUp", "keyUp"],
  // Focus events
  ["onFocus", "focus"],
  ["onBlur", "blur"],
  // Scroll events
  ["onScroll", "scroll"],
] as const

const EVENT_PROP_NAMES = new Set<string>(EVENT_PROPS.map(([name]) => name))

function syncEventListeners(container: Container, id: number, props: Props): void {
  for (const [propName, eventType] of EVENT_PROPS) {
    const handler = props[propName]
    if (handler) {
      registerEventHandler(container.eventHandlers, id, eventType, handler)
      container.renderer.setEventListener(id, eventType, true)
    }
  }
}

function diffEventListeners(
  container: Container,
  id: number,
  oldProps: Props,
  newProps: Props
): void {
  for (const [propName, eventType] of EVENT_PROPS) {
    const oldHandler = oldProps[propName]
    const newHandler = newProps[propName]

    if (oldHandler && !newHandler) {
      unregisterEventHandler(container.eventHandlers, id, eventType)
      container.renderer.setEventListener(id, eventType, false)
    } else if (newHandler && newHandler !== oldHandler) {
      registerEventHandler(container.eventHandlers, id, eventType, newHandler)
      if (!oldHandler) {
        container.renderer.setEventListener(id, eventType, true)
      }
    }
  }
}

// ── Style helper ─────────────────────────────────────────────────────

/**
 * The style an element should have, from all of its style sources.
 *
 * Every place that sends a style to the renderer goes through here. When a
 * source is added, one edit covers all of them. The previous code repeated
 * `props.style` at each call site, and `hideInstance` did not repeat it, so
 * hiding an element dropped its style.
 */
function computeStyle(props: Props, container: Container): StyleDesc {
  if (props.className && !container.classNames) {
    warnAboutMissingResolver(container)
    return props.style ?? {}
  }
  return withInlineStyle(styleForClassName(props.className, container.classNames), props.style)
}

/// One warning per root. A `className` with no resolver is a setup mistake, and
/// repeating it once per element per commit would bury everything else.
function warnAboutMissingResolver(container: Container): void {
  if (container.warnedAboutClassName) return
  container.warnedAboutClassName = true
  console.warn(
    "GPUIX: an element has a `className` but this root has no resolver. " +
      "Pass one to createRoot, such as createRoot(renderer, { resolveClassName })."
  )
}

function sendStyle(container: Container, id: number, props: Props): void {
  const style = computeStyle(props, container)
  if (Object.keys(style).length === 0) return
  container.renderer.setStyle(id, style)
}

// ── Custom prop forwarding ───────────────────────────────────────────

// Props that are handled by the reconciler directly (not forwarded as custom props).
const RESERVED_PROPS = new Set(["style", "className", "children", "key", "ref"])

// Built-in element types that don't use custom props.
const BUILT_IN_TYPES = new Set(["div", "text"])

// Props that reach Rust on EVERY element type, including div and text.
// Custom props are otherwise skipped for built-ins.
const UNIVERSAL_PROPS = new Set([
  "autoFocus",
  "tabIndex",
  "motion",
  "testId",
  // `highlight` is scoped by where it sits in the tree, so it has to reach a
  // plain `div`. Without it here, custom props are dropped for built-ins and
  // the prop silently never arrives in Rust.
  "highlight",
])

function isReservedProp(name: string): boolean {
  return RESERVED_PROPS.has(name) || EVENT_PROP_NAMES.has(name)
}

function serializeCustomProp(
  _type: string,
  _key: string,
  value: object | string | number | boolean | null | undefined
): string | object | number | boolean | null {
  if (value === undefined || typeof value === "function") return null
  return value
}

/** Send all custom props to Rust for non-built-in element types. */
function syncCustomProps(
  renderer: MutationRenderer,
  id: number,
  type: string,
  props: Props
): void {
  const builtIn = BUILT_IN_TYPES.has(type)
  for (const [key, value] of Object.entries(props)) {
    if (isReservedProp(key)) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    renderer.setCustomProp(id, key, serializeCustomProp(type, key, value))
  }
}

/** Diff and send changed custom props to Rust. */
function diffCustomProps(
  renderer: MutationRenderer,
  id: number,
  type: string,
  oldProps: Props,
  newProps: Props
): void {
  const builtIn = BUILT_IN_TYPES.has(type)
  const oldEntries = Object.entries(oldProps)
  const newKeys = Object.keys(newProps)
  // Updated or added props
  for (const [key, value] of Object.entries(newProps)) {
    if (isReservedProp(key)) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    const oldValue = oldEntries.find(([oldKey]) => oldKey === key)?.[1]
    if (oldValue !== value) {
      renderer.setCustomProp(id, key, serializeCustomProp(type, key, value))
    }
  }
  // Removed props
  for (const key of Object.keys(oldProps)) {
    if (isReservedProp(key)) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    if (!newKeys.includes(key)) {
      renderer.setCustomProp(id, key, null)
    }
  }
}

/**
 * Materialize a render-phase host node only after React places its subtree in
 * the commit phase. Abandoned concurrent renders stay as collectable JS
 * objects and never enter the native mutation queue.
 */
function materialize(node: HostNode): HostNodeState {
  const state = stateFor(node)
  if (state.mounted) return state

  const renderer = state.container.renderer
  if ("type" in node) {
    renderer.createElement(node.id, node.type)
    sendStyle(state.container, node.id, node.props)
    syncEventListeners(state.container, node.id, node.props)
    syncCustomProps(renderer, node.id, node.type, node.props)
  } else {
    renderer.createElement(node.id, "text")
    renderer.setText(node.id, node.text)
  }
  state.mounted = true

  for (const child of state.initialChildren) {
    materialize(child)
    renderer.appendChild(node.id, child.id)
  }
  state.initialChildren.length = 0
  return state
}

// ── Host config ──────────────────────────────────────────────────────

export const hostConfig = {
  supportsMutation: true,
  supportsPersistence: false,
  supportsHydration: false,

  // React creates host nodes while rendering and may abandon that work in
  // concurrent mode. Keep the description in JS; materialize it only from a
  // commit-phase placement callback.
  createInstance(
    type: ElementType,
    props: Props,
    rootContainerInstance: Container,
    _hostContext: HostContext
  ): Instance {
    const instance: Instance = { id: nextId(rootContainerInstance), type, props }
    hostNodeStates.set(instance, {
      container: rootContainerInstance,
      initialChildren: [],
      mounted: false,
    })
    return instance
  },

  appendChild(parent: Instance, child: Instance | TextInstance): void {
    const parentState = materialize(parent)
    materialize(child)
    parentState.container.renderer.appendChild(parent.id, child.id)
  },

  // React only calls this from the deletion path, never to move a node, so the
  // child is gone for good and has to be freed here. Detaching alone leaked
  // every removed text node: `detachDeletedInstance` runs for host components
  // only, so nothing else would ever destroy a `HostText`.
  removeChild(parent: Instance, child: Instance | TextInstance): void {
    const container = containerFor(parent)
    const destroyed = container.renderer.destroyElement(child.id)
    for (const id of destroyed) {
      unregisterEventHandlers(container.eventHandlers, id)
    }
  },

  insertBefore(
    parent: Instance,
    child: Instance | TextInstance,
    beforeChild: Instance | TextInstance
  ): void {
    const parentState = materialize(parent)
    materialize(child)
    parentState.container.renderer.insertBefore(parent.id, child.id, beforeChild.id)
  },

  insertInContainerBefore(
    _parent: Container,
    _child: Instance,
    _beforeChild: Instance
  ): void {},

  removeChildFromContainer(parent: Container, child: Instance): void {
    const destroyed = parent.renderer.destroyElement(child.id)
    for (const id of destroyed) {
      unregisterEventHandlers(parent.eventHandlers, id)
    }
  },

  prepareForCommit(_containerInfo: Container): Record<string, unknown> | null {
    return null
  },

  // Batch flush point: flushMutations() sends all queued mutations to Rust
  // in a single applyBatch() FFI call. This is the end of React's synchronous
  // commit phase — all mutations from this render are flushed together.
  resetAfterCommit(containerInfo: Container): void {
    containerInfo.renderer.flushMutations()
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
    rootContainerInstance: Container,
    _hostContext: HostContext
  ): TextInstance {
    const instance: TextInstance = {
      id: nextId(rootContainerInstance),
      text,
      parentId: null,
    }
    hostNodeStates.set(instance, {
      container: rootContainerInstance,
      initialChildren: [],
      mounted: false,
    })
    return instance
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
    const container = containerFor(instance)
    // Always resend style — per-element JSON is small, and this avoids
    // bugs from same-reference mutations or style removal.
    container.renderer.setStyle(instance.id, computeStyle(newProps, container))
    diffEventListeners(container, instance.id, oldProps, newProps)
    // Custom prop diff (for non-div/text elements)
    diffCustomProps(container.renderer, instance.id, instance.type, oldProps, newProps)
    instance.props = newProps
  },

  commitTextUpdate(
    textInstance: TextInstance,
    _oldText: string,
    newText: string
  ): void {
    rendererFor(textInstance).setText(textInstance.id, newText)
    textInstance.text = newText
  },

  appendChildToContainer(container: Container, child: Instance): void {
    materialize(child)
    container.renderer.setRoot(child.id)
  },

  appendInitialChild(parent: Instance, child: Instance | TextInstance): void {
    stateFor(parent).initialChildren.push(child)
  },

  hideInstance(instance: Instance): void {
    // Keep the element's own style. `visibility: hidden` skips the paint and
    // keeps the layout box, so replacing the whole style here would collapse
    // the box and lose every other style source on the element.
    //
    // The pseudo-selector styles go, because a hidden element must stay
    // hidden. A hover style that sets `visibility` would otherwise paint an
    // element React asked to hide.
    const container = containerFor(instance)
    const {
      hover: _hover,
      active: _active,
      selectors: _selectors,
      ...base
    } = computeStyle(instance.props, container)
    container.renderer.setStyle(instance.id, { ...base, visibility: "hidden" })
  },

  unhideInstance(instance: Instance, props: Props): void {
    const container = containerFor(instance)
    container.renderer.setStyle(instance.id, computeStyle(props, container))
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
  HostTransitionContext: createContext(null),
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
    const container = containerFor(instance)
    const destroyed = container.renderer.destroyElement(instance.id)
    for (const id of destroyed) {
      unregisterEventHandlers(container.eventHandlers, id)
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
