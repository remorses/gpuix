/// Host config for React's reconciler — mutation-based protocol.
///
/// Each reconciler callback (createInstance, appendChild, commitUpdate, etc.)
/// makes a direct napi call to the Rust retained tree. No JSON serialization
/// of the full element tree. Only changed elements cross the FFI boundary.

import { createContext } from "react"
import { DefaultEventPriority } from "react-reconciler/constants.js"
import type { HostEventHandler } from "@gpuix/native/host"
import {
  BUILT_IN_TYPES,
  handlerEventType,
  isReservedProp,
  RESERVED_PROPS,
  sameMutationValue,
  serializeCustomProp,
  UNIVERSAL_PROPS,
} from "@gpuix/native/host"

const NoEventPriority = 0
import type {
  Container,
  ElementType,
  HostContext,
  ImgInstance,
  Instance,
  MutationRenderer,
  Props,
  PublicInstance,
  TextInstance,
} from "../types/host.js"
import {
  registerEventHandler,
  unregisterEventHandler,
  unregisterEventHandlers,
} from "./event-registry.js"

let currentUpdatePriority = NoEventPriority

type HostNode = Instance | TextInstance

interface HostNodeState {
  container: Container
  initialChildren?: HostNode[]
  mounted: boolean
  /**
   * React hid this element (Suspense, Activity). Its native style carries
   * `visibility: hidden`, so a style update must keep that rather than
   * overwrite it with the visible one.
   */
  hidden: boolean
}

const hostNodeStates = new WeakMap<HostNode, HostNodeState>()

function attachHostMethods(instance: Instance): void {
  if (instance.scrollIntoView) return
  const container = hostNodeStates.get(instance)?.container
  if (!container) return
  instance.scrollIntoView = () => {
    container.nativeRenderer.scrollIntoView?.(instance.id)
  }
  if (instance.type !== "img") return
  const img = instance as ImgInstance
  img.setImage = (bytes) => {
    container.nativeRenderer.setImage?.(img.id, bytes as Buffer)
  }
  img.setImagePixels = (width, height, pixels, options) => {
    container.nativeRenderer.setImagePixels?.(
      img.id,
      width,
      height,
      pixels as Buffer,
      options?.format,
    )
  }
}

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

function syncEventListeners(container: Container, id: number, props: Props): void {
  const record = props as Record<string, unknown>
  for (const propName in record) {
    const handler = record[propName]
    const eventType = handlerEventType(propName, handler)
    if (eventType) {
      registerEventHandler(
        container.eventHandlers,
        id,
        eventType,
        handler as HostEventHandler
      )
      container.renderer.setEventListener(id, eventType, true)
    }
  }
}

/**
 * Walks the props an element has, not every event GPUIX knows: this runs for
 * each re-rendered host element, and most have a few props and no handlers.
 */
function diffEventListeners(
  container: Container,
  id: number,
  oldProps: Props,
  newProps: Props
): void {
  const previous = oldProps as Record<string, unknown>
  const next = newProps as Record<string, unknown>
  for (const propName in next) {
    const oldHandler = previous[propName]
    const newHandler = next[propName]
    const oldType = handlerEventType(propName, oldHandler)
    const newType = handlerEventType(propName, newHandler)
    if (oldType && !newType) {
      unregisterEventHandler(container.eventHandlers, id, oldType)
      container.renderer.setEventListener(id, oldType, false)
    } else if (newType && newHandler !== oldHandler) {
      registerEventHandler(
        container.eventHandlers,
        id,
        newType,
        newHandler as HostEventHandler
      )
      if (!oldType) {
        container.renderer.setEventListener(id, newType, true)
      }
    }
  }
  for (const propName in previous) {
    if (propName in next) continue
    const eventType = handlerEventType(propName, previous[propName])
    if (eventType) {
      unregisterEventHandler(container.eventHandlers, id, eventType)
      container.renderer.setEventListener(id, eventType, false)
    }
  }
}

// ── Style helper ─────────────────────────────────────────────────────

const NO_STYLE = {}

function sendStyle(renderer: MutationRenderer, id: number, props: Props): void {
  const style = props.style
  if (style == null || Object.keys(style).length === 0) return
  renderer.setStyle(id, style)
}

/**
 * Whether two `style` props reach Rust as the same payload. An absent style and
 * an empty one are the same, and so are two objects holding the same values:
 * `style={{ ... }}` is a new object on every render, so reference equality
 * alone would resend every styled element on every commit.
 *
 * Like React DOM, this does not see a style object mutated in place and passed
 * again. Pass a new object when a style changes.
 */
function sameStyle(left: Props["style"], right: Props["style"]): boolean {
  return left === right || sameMutationValue(left ?? NO_STYLE, right ?? NO_STYLE)
}

/** The style a hidden element keeps: its own, minus the states, plus `visibility: hidden`. */
function hiddenStyle(style: Props["style"]) {
  // Hover and active go, because a hidden element must stay hidden. A hover
  // style that sets `visibility` would otherwise paint an element React
  // asked to hide.
  const { hover: _hover, active: _active, ...base } = style ?? {}
  return { ...base, visibility: "hidden" }
}

// ── Custom prop forwarding ───────────────────────────────────────────

// Props that are handled by the reconciler directly (not forwarded as custom props).
/** Send all custom props to Rust for non-built-in element types. */
function syncCustomProps(
  renderer: MutationRenderer,
  id: number,
  type: string,
  props: Props
): void {
  const builtIn = BUILT_IN_TYPES.has(type)
  for (const [key, value] of Object.entries(props)) {
    if (isReservedProp(key, value)) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    renderer.setCustomProp(id, key, serializeCustomProp(value))
  }
}

/**
 * Diff and send changed custom props to Rust.
 *
 * Compared by value after serialization: a handler or an inline object is new on
 * every render, and a function serializes to `null` either way, so resending it
 * would only make Rust parse and discard the same value again.
 *
 * `onTop` can switch between a handler and data across renders. Rust only holds
 * the data form, so the handler form counts as absent.
 */
function diffCustomProps(
  renderer: MutationRenderer,
  id: number,
  type: string,
  oldProps: Props,
  newProps: Props
): void {
  const builtIn = BUILT_IN_TYPES.has(type)
  const previous = oldProps as Record<string, unknown>
  const next = newProps as Record<string, unknown>
  // Updated or added props
  for (const key in next) {
    const value = next[key]
    const oldValue = previous[key]
    if (value === oldValue) continue
    if (RESERVED_PROPS.has(key)) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    const wasData = key in previous && !isReservedProp(key, oldValue)
    if (handlerEventType(key, value)) {
      if (wasData) renderer.setCustomProp(id, key, null)
      continue
    }
    const serialized = serializeCustomProp(value as Parameters<typeof serializeCustomProp>[0])
    const unchanged = wasData
      ? sameMutationValue(
          serializeCustomProp(oldValue as Parameters<typeof serializeCustomProp>[0]),
          serialized
        )
      : serialized === null
    if (unchanged) continue
    renderer.setCustomProp(id, key, serialized)
  }
  // Removed props
  for (const key in previous) {
    if (key in next || isReservedProp(key, previous[key])) continue
    if (builtIn && !UNIVERSAL_PROPS.has(key)) continue
    renderer.setCustomProp(id, key, null)
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
    if (state.hidden) renderer.setStyle(node.id, hiddenStyle(node.props.style))
    else sendStyle(renderer, node.id, node.props)
    syncEventListeners(state.container, node.id, node.props)
    syncCustomProps(renderer, node.id, node.type, node.props)
  } else {
    renderer.createElement(node.id, "text")
    renderer.setText(node.id, node.text)
  }
  state.mounted = true

  if (state.initialChildren) {
    for (const child of state.initialChildren) {
      materialize(child)
      renderer.appendChild(node.id, child.id)
    }
    state.initialChildren = undefined
  }
  return state
}

const portalContainers = new WeakMap<Instance, Container>()

/**
 * The container a portal into `target` mounts through.
 *
 * Cached per target because React compares a portal's `containerInfo` by
 * identity, and a new object on every render would unmount and remount the
 * portal's whole subtree.
 */
export function portalContainerFor(target: Instance): Container {
  let container = portalContainers.get(target)
  if (!container) {
    container = Object.assign(Object.create(stateFor(target).container) as Container, {
      portalTarget: target,
    })
    portalContainers.set(target, container)
  }
  return container
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
    const instance: Instance = {
      id: nextId(rootContainerInstance),
      type,
      props,
    }
    hostNodeStates.set(instance, {
      container: rootContainerInstance,
      mounted: false,
      hidden: false,
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

  // A window's root has one child, so there is nothing to order. A portal's
  // children share their target with whatever else it holds, and React places
  // one before another whenever their keys reorder.
  insertInContainerBefore(
    container: Container,
    child: Instance,
    beforeChild: Instance
  ): void {
    const target = container.portalTarget
    if (!target) return
    materialize(child)
    container.renderer.insertBefore(target.id, child.id, beforeChild.id)
  },

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
      mounted: false,
      hidden: false,
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
    const state = stateFor(instance)
    const { container } = state
    // `commitUpdate` runs for every host element whose props object is new,
    // which is every element a parent re-rendered. Most of those did not change
    // their style, so send it only when it did.
    if (!sameStyle(oldProps.style, newProps.style)) {
      container.renderer.setStyle(
        instance.id,
        state.hidden ? hiddenStyle(newProps.style) : (newProps.style ?? {})
      )
    }
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
    const target = container.portalTarget
    if (target) {
      materialize(target)
      container.renderer.appendChild(target.id, child.id)
    } else {
      container.renderer.setRoot(child.id)
    }
  },

  appendInitialChild(parent: Instance, child: Instance | TextInstance): void {
    const state = stateFor(parent)
    state.initialChildren ??= []
    state.initialChildren.push(child)
  },

  hideInstance(instance: Instance): void {
    // Keep the element's own style. `visibility: hidden` skips the paint and
    // keeps the layout box, so replacing the whole style here would collapse
    // the box and lose every other style on the element.
    stateFor(instance).hidden = true
    rendererFor(instance).setStyle(instance.id, hiddenStyle(instance.props.style))
  },

  unhideInstance(instance: Instance, props: Props): void {
    stateFor(instance).hidden = false
    rendererFor(instance).setStyle(instance.id, props.style ?? {})
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
    attachHostMethods(instance)
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
